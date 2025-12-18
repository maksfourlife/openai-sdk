#[macro_export]
macro_rules! define_ids {
    ($($id:ident),*) => {
        $(
            paste::paste! {
                #[derive(
                    Debug,
                    Clone,
                    Default,
                    PartialEq,
                    Eq,
                    ::derive_more::From,
                    ::derive_more::Into,
                    ::derive_more::Display,
                    ::serde::Deserialize,
                    ::serde::Serialize
                )]
                pub struct $id(pub String);

                #[derive(
                    Debug,
                    PartialEq,
                    Eq,
                    ::derive_more::Display,
                    ::serde::Serialize
                )]
                pub struct [<$id Ref>](pub str);

                impl<'a> From<&'a str> for &'a [<$id Ref>] {
                    fn from(value: &'a str) -> Self {
                        unsafe { std::mem::transmute::<&str, &[<$id Ref>]>(value) }
                    }
                }

                impl<'a> From<&'a [<$id Ref>]> for &'a str {
                    fn from(value: &'a [<$id Ref>]) -> Self {
                        unsafe { std::mem::transmute::<&[<$id Ref>], &str>(value) }
                    }
                }

                impl AsRef<[<$id Ref>]> for $id {
                    fn as_ref(&self) -> &[<$id Ref>] {
                        (&self.0 as &str).into()
                    }
                }
            }
        )*
    };
}
