macro_rules! validated_string {
    (
        $(#[$meta:meta])*
        $name:ident, $error:ty, $validate:path
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, serde::Deserialize)]
        #[serde(try_from = "String")]
        pub struct $name(String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = $error;

            fn try_from(text: String) -> Result<Self, Self::Error> {
                $validate(&text)?;
                Ok(Self(text))
            }
        }

        impl std::str::FromStr for $name {
            type Err = $error;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                Self::try_from(text.to_owned())
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
