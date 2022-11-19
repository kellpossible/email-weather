use std::{collections::HashMap, hash::Hash, marker::PhantomData};

use serde::{de::Visitor, Deserialize};

#[derive(Debug)]
pub struct LevelVariable<L, LF, T> {
    pub values: HashMap<L, T>,
    level_kind: PhantomData<LF>,
}

impl<L, LF, T> LevelVariable<L, LF, T> {
    pub fn new(values: HashMap<L, T>) -> Self {
        Self {
            values,
            level_kind: PhantomData,
        }
    }
}

impl<'de, L, LF, T> Deserialize<'de> for LevelVariable<L, LF, T>
where
    LF: LevelField<L>,
    L: Level + Eq + Hash,
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(LevelStructVisitor::new())
    }
}

pub trait Level: Sized {
    fn enumerate() -> Vec<Self>;
}

pub trait LevelField<L> {
    fn name(level: &L) -> String;
}

struct LevelStructField<L, LF, T> {
    level: L,
    level_field_type: PhantomData<LF>,
    data_type: PhantomData<T>,
}

impl<L, LF, T> LevelStructField<L, LF, T> {
    fn new(level: L) -> Self {
        Self {
            level,
            level_field_type: PhantomData,
            data_type: PhantomData,
        }
    }
}

impl<'de, L, LF, T> Deserialize<'de> for LevelStructField<L, LF, T>
where
    L: Level,
    LF: LevelField<L>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_identifier(LevelStructFieldVisitor::<L, LF, T>::new())
    }
}

struct LevelStructFieldVisitor<L, LF, T>(PhantomData<L>, PhantomData<LF>, PhantomData<T>);

impl<L, LF, T> LevelStructFieldVisitor<L, LF, T> {
    pub fn new() -> Self {
        Self(PhantomData, PhantomData, PhantomData)
    }
}

impl<'de, L, LF, T> Visitor<'de> for LevelStructFieldVisitor<L, LF, T>
where
    L: Level,
    LF: LevelField<L>,
{
    type Value = LevelStructField<L, LF, T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        let variants = L::enumerate();

        let variant = variants
            .get(0)
            .expect("Expected level to have at least one variant");

        let field_name = LF::name(&variant);
        formatter.write_fmt(format_args!(
            "Expected something in the format of: `{}`",
            field_name
        ))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        L::enumerate()
            .into_iter()
            .map(|level| {
                let name = LF::name(&level);
                (level, name)
            })
            .find(|(_, field_name)| field_name == v)
            .ok_or_else(|| serde::de::Error::custom(format!("Unexpected field: {}", v)))
            .map(|(level, _)| LevelStructField::new(level))
    }
}

struct LevelStructVisitor<L, LF, T>(PhantomData<L>, PhantomData<LF>, PhantomData<T>);

impl<L, LF, T> LevelStructVisitor<L, LF, T> {
    pub fn new() -> Self {
        Self(PhantomData, PhantomData, PhantomData)
    }
}

impl<'de, L, LF, T> Visitor<'de> for LevelStructVisitor<L, LF, T>
where
    L: Level + Eq + Hash,
    LF: LevelField<L>,
    T: Deserialize<'de>,
{
    type Value = LevelVariable<L, LF, T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("Expecting map of level field names to level field values")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut values: HashMap<L, T> = HashMap::with_capacity(map.size_hint().unwrap_or(0));
        while let Some(key) = map.next_key::<LevelStructField<L, LF, T>>()? {
            let value = map.next_value::<T>()?;
            values.insert(key.level, value);
        }

        Ok(LevelVariable::new(values))
    }
}

#[cfg(test)]
mod test {
    use std::fmt::Display;

    use serde_json::json;

    use super::{Level, LevelField, LevelVariable};

    #[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
    enum TestLevel {
        One,
        Two,
    }

    impl Display for TestLevel {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(match self {
                TestLevel::One => "one",
                TestLevel::Two => "two",
            })
        }
    }

    impl Level for TestLevel {
        fn enumerate() -> Vec<Self> {
            vec![Self::One, Self::Two]
        }
    }

    struct TestLevelField;

    impl LevelField<TestLevel> for TestLevelField {
        fn name(level: &TestLevel) -> String {
            format!("test_{}", level)
        }
    }

    #[test]
    fn test_deserialize_level_variable() {
        let value = json!({
            "test_one": 1,
            "test_two": 2,
        });

        let variable: LevelVariable<TestLevel, TestLevelField, u64> =
            serde_json::from_value(value).unwrap();
        assert_eq!(1, *variable.values.get(&TestLevel::One).unwrap());
        assert_eq!(2, *variable.values.get(&TestLevel::Two).unwrap());
        assert_eq!(2, variable.values.len());
    }
}
