pub(crate) mod serde;

use ::serde::{Deserialize, Serialize};

use super::TableId;
use crate::datastore::TxOffset;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct RowData(pub Arc<[u8]>);

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct InsertList {
    pub table_id: TableId,
    pub inserts: Arc<[RowData]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct DeleteList {
    pub table_id: TableId,
    pub deletes: Arc<[RowData]>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub struct TxData {
    pub inserts: Arc<[InsertList]>,
    pub deletes: Arc<[DeleteList]>,
    pub truncs: Arc<[TableId]>,
}

#[derive(Debug, Clone)]
pub struct TxResult {
    pub tx_offset: TxOffset,
    pub tx_data: TxData,
}
