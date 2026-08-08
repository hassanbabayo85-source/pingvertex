use crate::enums::{AssertionOperator, ResponsePart};
use crate::shared::ids::AssertionId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assertion {
    pub id: AssertionId,
    pub response_part: ResponsePart,
    pub operator: AssertionOperator,
    pub target: Option<String>,
    pub expected_value: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Assertion {
    pub fn new(
        response_part: ResponsePart,
        operator: AssertionOperator,
        target: Option<String>,
        expected_value: Option<String>,
    ) -> Result<Self, String> {
        let now = Utc::now();
        let assertion = Self {
            id: AssertionId::new(),
            response_part,
            operator,
            target,
            expected_value,
            created_at: now,
            updated_at: now,
        };

        assertion.validate()?;
        Ok(assertion)
    }

    pub fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}
