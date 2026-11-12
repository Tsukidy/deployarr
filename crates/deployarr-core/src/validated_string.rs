macro_rules! validated_string {
    (
        $(#[$meta:meta])*
        $name:ident, $error:ty, $validate:path
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, serde::Deserialize)]
        #[serde(try_from = "alloc::string::String")]
        pub struct $name(alloc::string::String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<alloc::string::String> for $name {
            type Error = $error;

            fn try_from(text: alloc::string::String) -> Result<Self, Self::Error> {
                $validate(&text)?;
                Ok(Self(text))
            }
        }

        impl core::str::FromStr for $name {
            type Err = $error;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                Self::try_from(alloc::borrow::ToOwned::to_owned(text))
            }
        }

        impl core::fmt::Display for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }
    };
}

pub(crate) use validated_string;
