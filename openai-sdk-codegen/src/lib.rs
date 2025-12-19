#![allow(clippy::single_match, clippy::collapsible_match)]

use std::{borrow::Cow, collections::HashSet, env::VarError, path::PathBuf};

use convert_case::ccase;
use openapiv3::{
    AnySchema, ArrayType, Components, ObjectType, OpenAPI, ReferenceOr, Schema, SchemaKind,
    StringType, Type,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use snafu::{ResultExt, Snafu};
use syn::{Ident, Lit, parse_macro_input, parse_str};

#[proc_macro]
pub fn generate(tokens: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let lit = parse_macro_input!(tokens as Lit);
    match try_generate(&lit) {
        Ok(ok) => ok.into(),
        Err(err) => syn::Error::new_spanned(lit, err)
            .into_compile_error()
            .into(),
    }
}

#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("Expected string literal"))]
    ExpectedStringLiteral,
    #[snafu(display("Could not fetch CARGO_MANIFEST_DIR: {source}"))]
    Var { source: VarError },
    #[snafu(display("Could not read file at \"{}\": {source}", path.display()))]
    ReadFile {
        source: std::io::Error,
        path: PathBuf,
    },
    #[snafu(display("Could not deserialize OpenAPI: {source}"))]
    DeserializeOpenAPI { source: serde_yaml::Error },
    #[snafu(display("Invalid reference: {reference}"))]
    InvalidReference { reference: String },
    #[snafu(transparent)]
    Syn { source: syn::Error },
}

fn try_generate(lit: &Lit) -> Result<TokenStream, Error> {
    let s = match &lit {
        Lit::Str(s) => s.value(),
        _ => return Err(Error::ExpectedStringLiteral),
    };

    let manifest_dir: PathBuf = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|err| Error::Var { source: err })?
        .into();

    let content_path = manifest_dir.join(s);
    let content = std::fs::read(&content_path).context(ReadFileSnafu { path: content_path })?;

    let openapi: OpenAPI = serde_yaml::from_slice(&content)
        .map_err(|err| Error::DeserializeOpenAPI { source: err })?;

    let mut outputs = vec![];
    let mut nullable = HashSet::new();

    if let Some(components) = &openapi.components {
        for (schema_name, ref_or_schema) in &components.schemas {
            let ReferenceOr::Item(schema) = ref_or_schema else {
                continue;
            };

            expand_schema(&mut outputs, &mut nullable, components, schema_name, schema)?;
        }
    }

    let quote = quote! {
        #(#outputs)*
    };

    Ok(quote)
}

fn expand_schema(
    outputs: &mut Vec<TokenStream>,
    nullable: &mut HashSet<String>,
    _components: &Components,
    schema_name: &str,
    schema: &Schema,
) -> Result<(), Error> {
    let ident = format_ident!("{}", format_struct_name(schema_name));

    let doc = build_schema_doc(schema);

    match &schema.schema_kind {
        SchemaKind::Type(r#type) => match r#type {
            Type::String(string) => {
                expand_string(outputs, &ident, &doc, string)?;
            }
            Type::Object(object) => {
                expand_object(outputs, object, &ident, doc)?;
            }
            Type::Array(array) => {
                expand_array(outputs, nullable, _components, &ident, &doc, array)?;
            }
            Type::Boolean(_) => {
                outputs.push(quote! {
                    #(#doc)*
                    pub type #ident = bool;
                });
            }
            _ => {}
        },
        SchemaKind::AnyOf { any_of } => {
            expand_any_of(outputs, nullable, schema_name, doc, any_of)?;
        }
        SchemaKind::AllOf { all_of } => {
            expand_all_of(outputs, &ident, all_of)?;
        }
        SchemaKind::Any(any) => {
            if !any.any_of.is_empty() {
                expand_any_of(outputs, nullable, schema_name, doc, &any.any_of)?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn expand_string(
    outputs: &mut Vec<TokenStream>,
    ident: &Ident,
    doc: &[TokenStream],
    string: &StringType,
) -> Result<(), Error> {
    let variants: Vec<_> = string
        .enumeration
        .iter()
        .filter_map(|x| {
            x.as_ref().map(|x| {
                let ident = format_variant_name(x);
                quote! {
                    #[serde(rename = #x)]
                    #ident
                }
            })
        })
        .collect();

    outputs.push(quote! {
        #(#doc)*
        #[derive(Debug, ::serde::Deserialize, ::serde::Serialize)]
        pub enum #ident {
            #(#variants,)*
        }
    });

    Ok(())
}

fn expand_array(
    outputs: &mut Vec<TokenStream>,
    nullable: &mut HashSet<String>,
    _components: &Components,
    ident: &Ident,
    doc: &[TokenStream],
    array: &ArrayType,
) -> Result<(), Error> {
    if let Some(items) = &array.items {
        let item_type = match items {
            ReferenceOr::Reference { reference } => {
                format_ident!("{}", parse_reference(reference)?)
            }
            ReferenceOr::Item(schema) => {
                // TODO: expand struct
                let schema_name = format!("{ident}Item");

                expand_schema(outputs, nullable, _components, &schema_name, schema)?;

                format_ident!("{ident}Item")
            }
        };

        outputs.push(quote! {
            #(#doc)*
            pub type #ident = Vec<#item_type>;
        });
    }

    Ok(())
}

fn expand_any_of(
    outputs: &mut Vec<TokenStream>,
    nullable: &mut HashSet<String>,
    schema_name: &str,
    mut attrs: Vec<TokenStream>,
    any_of: &[ReferenceOr<Schema>],
) -> Result<(), Error> {
    let mut variants = vec![];

    for ref_or_schema in any_of {
        match ref_or_schema {
            ReferenceOr::Item(item) => {
                if is_null_object(item) {
                    nullable.insert(schema_name.to_string());
                    continue;
                }

                if let SchemaKind::Type(Type::String(string)) = &item.schema_kind
                    && !string.enumeration.is_empty()
                {
                    attrs.extend(build_schema_doc(item));

                    string
                        .enumeration
                        .iter()
                        .filter_map(|x| x.as_deref())
                        .for_each(|name| {
                            let ident = format_variant_name(name);
                            variants.push(quote! {
                                #[serde(rename = #name)]
                                #ident
                            });
                        });
                }

                // expand_schema(outputs, nullable, _components, schema_name, item)?;
                // TODO: expand schema
            }
            ReferenceOr::Reference { reference } => {
                let name = format_struct_name(parse_reference(reference)?);

                let var_name = format_ident!("{name}");
                let var_type = parse_str::<syn::Type>(&name)?;

                variants.push(quote! {
                    #var_name(#var_type)
                });
            }
        }
    }

    let ident = format_ident!("{}", format_struct_name(schema_name));

    outputs.push(quote! {
        #(#attrs)*
        #[derive(Debug, ::serde::Deserialize, ::serde::Serialize)]
        pub enum #ident {
            #(#variants,)*
        }
    });

    Ok(())
}

fn expand_all_of(
    outputs: &mut Vec<TokenStream>,
    ident: &Ident,
    all_of: &[ReferenceOr<Schema>],
) -> Result<(), Error> {
    let mut fields = vec![];

    for ref_or_schema in all_of {
        match ref_or_schema {
            ReferenceOr::Item(_) => {
                // TODO: expand schema
            }
            ReferenceOr::Reference { reference } => {
                let name = format_struct_name(parse_reference(reference)?);

                // TODO: prefix with r#
                let field_name = format_ident!("{}", ccase!(snake, &name));
                let field_type = parse_str::<syn::Type>(&name)?;

                fields.push(quote! {
                    #[serde(flatten)]
                    pub #field_name: #field_type
                });
            }
        }
    }

    outputs.push(quote! {
        #[derive(Debug, ::serde::Deserialize, ::serde::Serialize)]
        pub struct #ident {
            #(#fields,)*
        }
    });

    Ok(())
}

fn expand_object(
    outputs: &mut Vec<TokenStream>,
    object: &ObjectType,
    ident: &Ident,
    struct_attrs: Vec<TokenStream>,
) -> Result<(), Error> {
    struct Field<'a> {
        name: &'a str,
        r#type: Cow<'a, str>,
        nullable: bool,
        serializer: Option<&'static str>,
        attrs: Vec<TokenStream>,
    }

    let mut fields = vec![];

    for (field_name, prop) in &object.properties {
        match prop {
            ReferenceOr::Reference { reference } => fields.push(Field {
                name: field_name,
                r#type: Cow::Borrowed(parse_reference(reference)?),
                nullable: false,
                serializer: None,
                attrs: vec![],
            }),
            ReferenceOr::Item(item) => {
                let field_attrs = build_schema_doc(item);

                let is_timestamp =
                    matches!(&item.schema_data.description, Some(x) if x.contains("timestamp"));

                let field_type = match &item.schema_kind {
                    SchemaKind::Type(Type::String(string)) if string.enumeration.is_empty() => {
                        Some("String")
                    }
                    SchemaKind::Type(Type::Number(_)) => Some("f64"),
                    SchemaKind::Type(Type::Integer(integer)) => Some(if is_timestamp {
                        "::chrono::DateTime<::chrono::Utc>"
                    } else if integer.minimum > Some(0) {
                        "u64"
                    } else {
                        "i64"
                    }),
                    SchemaKind::Type(Type::Boolean(_)) => Some("bool"),
                    // TODO: array
                    _ => None,
                };

                let serializer = if is_timestamp
                    && matches!(&item.schema_kind, SchemaKind::Type(Type::Integer(_)))
                {
                    Some("serde_with::TimestampSeconds<i64>")
                } else {
                    None
                };

                // TODO: else expand struct

                if let Some(field_type) = field_type {
                    fields.push(Field {
                        name: field_name,
                        r#type: Cow::Borrowed(field_type),
                        nullable: item.schema_data.nullable,
                        serializer,
                        attrs: field_attrs,
                    })
                }
            }
        }

        // expand_schema(outputs, components, prop_name, reference_or_deref(prop))?;
    }

    fields.iter_mut().for_each(|field| {
        let required = object.required.iter().any(|x| x == field.name);
        field.nullable = field.nullable || !required;
    });

    let fields = fields
        .into_iter()
        .map(|field| {
            let field_type = if field.nullable {
                Cow::Owned(format!("Option<{}>", field.r#type))
            } else {
                field.r#type
            };

            let serializer = field.serializer.map(|serializer| {
                if field.nullable {
                    Cow::Owned(format!("Option<{serializer}>"))
                } else {
                    Cow::Borrowed(serializer)
                }
            });

            let field_name = format_field_name(field.name);
            let field_type = parse_str::<syn::Type>(&field_type)?;

            let mut field_attrs = field.attrs.clone();

            if let Some(serializer) = serializer {
                let with = format!("::serde_with::As::<{serializer}>");
                field_attrs.push(quote! {
                    #[serde(with = #with)]
                });
            }

            syn::Result::Ok(quote! {
                #(#field_attrs)*
                pub #field_name: #field_type
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let struct_quote = quote! {
        #(#struct_attrs)*
        #[derive(Debug, ::serde::Deserialize, ::serde::Serialize)]
        pub struct #ident {
            #(#fields,)*
        }
    };

    outputs.push(struct_quote);

    Ok(())
}

fn build_schema_doc(schema: &Schema) -> Vec<TokenStream> {
    let mut doc = vec![];

    if let Some(title) = schema.schema_data.title.as_deref() {
        doc.push(quote! { #[doc = #title] });
    }

    if let Some(description) = schema.schema_data.description.as_deref() {
        doc.push(quote! { #[doc = #description] });
    }

    doc
}

fn is_null_object(schema: &Schema) -> bool {
    match &schema.schema_kind {
        SchemaKind::Any(AnySchema { typ, .. }) => typ.as_deref() == Some("null"),
        _ => false,
    }
}

fn format_struct_name(value: &str) -> String {
    value.replace('-', "_")
}

fn format_variant_name(value: &str) -> Ident {
    let mut value = value
        .split('.')
        .map(|part| {
            part.split('-')
                .map(|part| ccase!(pascal, part))
                .collect::<Vec<_>>()
                .join("_")
        })
        .collect::<Vec<_>>()
        .join("_");
    if value.starts_with(['0', '1', '2', '3', '4', '5', '6', '7', '8', '9']) {
        value = format!("_{value}");
    }
    format_ident!("{value}")
}

fn format_field_name(value: &str) -> Ident {
    let value = value.replace('.', "_");
    match syn::parse_str::<syn::Ident>(&value) {
        Ok(ok) => ok,
        Err(_) => format_ident!("r#{value}"),
    }
}

fn parse_reference(value: &str) -> Result<&str, Error> {
    match value.rsplit_once('/') {
        Some(("#/components/schemas", name)) => Ok(name),
        _ => Err(Error::InvalidReference {
            reference: value.to_string(),
        }),
    }
}

// All anyOf schemas that have "type: null" in a list should not contain null
// variant and instead be added to nullable list
//
// Example:
//   ModelIdsCompaction:
//     anyOf:
//       - $ref: "#/components/schemas/ModelIdsResponses"
//       - type: string
//       - type: "null"
//
// Those who contain "type: string" and enum inside should be flattened
//
// Example:
//   anyOf:
//       - type: string
//           description: |
//           `auto` is the default value
//           enum:
//           - auto
//           x-stainless-const: true
//       - $ref: "#/components/schemas/ResponseFormatText"
//       - $ref: "#/components/schemas/ResponseFormatJsonObject"
//       - $ref: "#/components/schemas/ResponseFormatJsonSchema"

#[cfg(test)]
mod test {

    use proc_macro2::Span;

    use super::*;

    // #[test]
    // fn test_expand_chunking_strategy_request_param() {
    //     let yaml = r##"
    //         type: object
    //         description: >-
    //             The chunking strategy used to chunk the file(s). If not set, will use the `auto` strategy. Only
    //             applicable if `file_ids` is non-empty.
    //         anyOf:
    //             - $ref: "#/components/schemas/AutoChunkingStrategyRequestParam"
    //             - $ref: "#/components/schemas/StaticChunkingStrategyRequestParam"
    //         discriminator:
    //             propertyName: type
    //     "##;

    //     let schema = serde_yaml::from_str::<Schema>(yaml).unwrap();

    //     let SchemaKind::Any(any) = &schema.schema_kind else {
    //         panic!()
    //     };

    //     assert_eq!(
    //         schema.schema_data.discriminator,
    //         Some(Discriminator {
    //             property_name: "type".to_string(),
    //             ..Default::default()
    //         })
    //     );
    //     assert_eq!(any.typ, Some("object".to_string()));
    // }

    #[test]
    fn test_expand_string() {
        let yaml = r##"
            type: string
            description: The event type.
            enum:
                - api_key.created
                - api_key.updated
        "##;

        let schema = serde_yaml::from_str::<Schema>(yaml).unwrap();

        let SchemaKind::Type(Type::String(string)) = &schema.schema_kind else {
            panic!()
        };

        let mut outputs = vec![];
        let ident = Ident::new("Test", Span::call_site());

        expand_string(&mut outputs, &ident, &[], string).unwrap();

        let expected = quote! {
            #[derive(Debug, ::serde::Deserialize, ::serde::Serialize)]
            pub enum Test {
                #[serde(rename = "api_key.created")]
                ApiKey_Created,
                #[serde(rename = "api_key.updated")]
                ApiKey_Updated,
            }
        };

        assert_eq!(outputs[0].to_string(), expected.to_string());
    }

    #[test]
    fn test_expand_service_tier() {
        let yaml = r##"
            anyOf:
                - type: string
                  enum:
                        - auto
                        - default
                        - flex
                        - scale
                        - priority
                  default: auto
                - type: "null"
        "##;
        let schema = serde_yaml::from_str::<Schema>(yaml).unwrap();

        let SchemaKind::AnyOf { any_of } = &schema.schema_kind else {
            panic!();
        };

        let mut outputs = vec![];
        let mut nullable = HashSet::new();

        expand_any_of(&mut outputs, &mut nullable, "ServiceTier", vec![], any_of).unwrap();

        let expected = quote! {
            #[derive(Debug, ::serde::Deserialize, ::serde::Serialize)]
            pub enum ServiceTier {
                #[serde(rename = "auto")]
                Auto,
                #[serde(rename = "default")]
                Default,
                #[serde(rename = "flex")]
                Flex,
                #[serde(rename = "scale")]
                Scale,
                #[serde(rename = "priority")]
                Priority,
            }
        };

        assert_eq!(outputs[0].to_string(), expected.to_string());
    }
}
