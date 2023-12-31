// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

use super::{
    balance::{Balance, BalanceConnection},
    coin::CoinConnection,
    name_service::NameServiceConnection,
    object::{ObjectConnection, ObjectFilter},
    stake::StakeConnection,
    sui_address::SuiAddress,
    transaction_block::{TransactionBlockConnection, TransactionBlockFilter},
};

pub(crate) struct Address;

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub(crate) enum AddressTransactionBlockRelationship {
    Sign, // Transactions this address has signed
    Sent, // Transactions that transferred objects from this address
    Recv, // Transactions that received objects into this address
    Paid, // Transactions that were paid for by this address
}

#[allow(unreachable_code)]
#[allow(unused_variables)]
#[Object]
impl Address {
    async fn transaction_block_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        relation: Option<AddressTransactionBlockRelationship>,
        filter: Option<TransactionBlockFilter>,
    ) -> Option<TransactionBlockConnection> {
        unimplemented!()
    }

    // =========== Owner interface methods =============

    pub async fn location(&self) -> SuiAddress {
        unimplemented!()
    }

    pub async fn object_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<ObjectFilter>,
    ) -> Option<ObjectConnection> {
        unimplemented!()
    }

    pub async fn balance(&self, type_: Option<String>) -> Balance {
        unimplemented!()
    }

    pub async fn balance_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Option<BalanceConnection> {
        unimplemented!()
    }

    pub async fn coin_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        type_: Option<String>,
    ) -> Option<CoinConnection> {
        unimplemented!()
    }

    pub async fn stake_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Option<StakeConnection> {
        unimplemented!()
    }

    pub async fn default_name_service_name(&self) -> Option<String> {
        unimplemented!()
    }

    pub async fn name_service_connection(
        &self,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Option<NameServiceConnection> {
        unimplemented!()
    }
}
