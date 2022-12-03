use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::Hash,
    marker::PhantomData,
};

use serde::{de::Visitor, Deserialize};

pub struct LevelVariable<L, LF, T> {
    values: HashMap<L, T>,
    level_kind: PhantomData<LF>,
}

impl<L, LF, T> Debug for LevelVariable<L, LF, T>
where
    L: Debug,
    T: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LevelVariable")
            .field("values", &self.values)
            .finish()
    }
}

impl<L, LF, T> Clone for LevelVariable<L, LF, T>
where
    L: Clone,
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            values: self.values.clone(),
            level_kind: self.level_kind,
        }
    }
}

impl<L, LF, T> LevelVariable<L, LF, T> {
    pub fn new(values: HashMap<L, T>) -> Self {
        Self {
            values,
            level_kind: PhantomData,
        }
    }
}

impl<L, LF, T> LevelVariable<L, LF, T>
where
    L: Hash + Eq,
{
    pub fn value(&self, level: &L) -> Option<&T> {
        self.values.get(level)
    }
}

impl<L, LF, T> Default for LevelVariable<L, LF, T> {
    fn default() -> Self {
        Self::new(HashMap::default())
    }
}

impl<'de, L, LF, T> Deserialize<'de> for LevelVariable<L, LF, T>
where
    LF: LevelField<L>,
    L: Level + Hash + Eq,
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(LevelStructVisitor::new())
    }
}

pub trait Level: Sized + Clone + 'static {
    fn enumerate() -> &'static [Self];
}

pub trait LevelField<L: Level> {
    fn name(level: &L) -> &'static str;
    fn enumerate_names() -> HashSet<&'static str> {
        L::enumerate().iter().map(Self::name).collect()
    }
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

        let field_name = LF::name(variant);
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
            .iter()
            .cloned()
            .map(|level| {
                let field_name = LF::name(&level);
                (level, field_name)
            })
            .find(|(_, field_name)| *field_name == v)
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
    use std::{collections::HashMap, fmt::Display};

    use once_cell::sync::Lazy;
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
        fn enumerate() -> &'static [Self] {
            &[Self::One, Self::Two]
        }
    }

    struct TestLevelField;

    static TEST_LEVEL_FIELD_NAMES: Lazy<HashMap<TestLevel, String>> = Lazy::new(|| {
        TestLevel::enumerate()
            .iter()
            .cloned()
            .map(|level| (level, format!("test_{}", level)))
            .collect()
    });

    impl LevelField<TestLevel> for TestLevelField {
        fn name(level: &TestLevel) -> &'static str {
            TEST_LEVEL_FIELD_NAMES.get(level).unwrap()
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
