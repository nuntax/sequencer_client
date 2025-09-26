use crate::types::consensus::transactions::ArbTxEnvelope;

pub mod batchpostingreport;

#[derive(Debug)]
pub enum Message {
    Transaction(ArbTxEnvelope),
    BatchPostingReport(batchpostingreport::BatchPostingReport),
}
