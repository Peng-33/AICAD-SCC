use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposalEntity {
    pub id: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposalRelation {
    pub kind: String,
    pub subject: String,
    pub reference: String,
    #[serde(default)]
    pub axis: Option<String>,
    #[serde(default)]
    pub value: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposalDimension {
    pub target: String,
    pub kind: String,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposalArray {
    pub id: String,
    pub kind: String,
    pub count: u32,
    #[serde(default)]
    pub spacing: Option<f64>,
    pub element: String,
    #[serde(default)]
    pub rows: Option<u32>,
    #[serde(default)]
    pub cols: Option<u32>,
    #[serde(default)]
    pub row_spacing: Option<f64>,
    #[serde(default)]
    pub col_spacing: Option<f64>,
    #[serde(default)]
    pub radius: Option<f64>,
    #[serde(default)]
    pub direction: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposalAnchor {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub entity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    pub version: String,
    pub entities: Vec<ProposalEntity>,
    pub relations: Vec<ProposalRelation>,
    pub dimensions: Vec<ProposalDimension>,
    pub arrays: Vec<ProposalArray>,
    pub anchors: Vec<ProposalAnchor>,
    pub metadata: Value,
}
