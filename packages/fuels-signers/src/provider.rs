#[cfg(feature = "fuel-core")]
use fuel_core::service::{Config, FuelService};
use fuel_gql_client::client::schema::coin::Coin;
use fuel_gql_client::client::types::TransactionResponse;
use fuel_gql_client::client::{
    schema, FuelClient, PageDirection, PaginatedResult, PaginationRequest,
};
use fuel_tx::Receipt;
use fuel_tx::{Address, AssetId, Input, Output, Transaction};
use fuel_vm::consts::REG_ONE;
use std::io;
use std::net::SocketAddr;

use fuel_vm::prelude::Opcode;
use fuels_core::errors::Error;
use fuels_core::parameters::TxParameters;
use thiserror::Error;

/// An error involving a signature.
#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("Request failed: {0}")]
    TransactionRequestError(String),
    #[error(transparent)]
    ClientRequestError(#[from] io::Error),
}

/// Encapsulates common client operations in the SDK.
/// Note that you may also use `client`, which is an instance
/// of `FuelClient`, directly, which providers a broader API.
#[derive(Debug, Clone)]
pub struct Provider {
    pub client: FuelClient,
}

impl Provider {
    pub fn new(client: FuelClient) -> Self {
        Self { client }
    }

    /// Shallow wrapper on client's submit.
    pub async fn send_transaction(&self, tx: &Transaction) -> io::Result<Vec<Receipt>> {
        let tx_id = self.client.submit(tx).await?;

        self.client.receipts(&tx_id.0.to_string()).await
    }

    #[cfg(feature = "fuel-core")]
    /// Launches a local `fuel-core` network based on provided config.
    pub async fn launch(config: Config) -> Result<FuelClient, Error> {
        let srv = FuelService::new_node(config).await.unwrap();
        Ok(FuelClient::from(srv.bound_address))
    }

    /// Connects to an existing node at the given address
    pub async fn connect(socket: SocketAddr) -> Result<Provider, Error> {
        Ok(Self {
            client: FuelClient::from(socket),
        })
    }

    /// Shallow wrapper on client's coins API.
    pub async fn get_coins(&self, from: &Address) -> Result<Vec<Coin>, ProviderError> {
        let mut coins: Vec<Coin> = vec![];

        let mut cursor = None;

        loop {
            let res = self
                .client
                .coins(
                    &from.to_string(),
                    None,
                    PaginationRequest {
                        cursor: cursor.clone(),
                        results: 100,
                        direction: PageDirection::Forward,
                    },
                )
                .await?;

            if res.results.is_empty() {
                break;
            }
            coins.extend(res.results);
            cursor = res.cursor;
        }

        Ok(coins)
    }

    pub async fn get_spendable_coins(
        &self,
        from: &Address,
        asset_id: AssetId,
        amount: u64,
    ) -> io::Result<Vec<Coin>> {
        let res = self
            .client
            .coins_to_spend(
                &from.to_string(),
                vec![(format!("{:#x}", asset_id).as_str(), amount)],
                None,
                None,
            )
            .await?;

        Ok(res)
    }

    /// Craft a transaction used to transfer funds between two addresses.
    pub fn build_transfer_tx(
        &self,
        inputs: &[Input],
        outputs: &[Output],
        params: TxParameters,
    ) -> Transaction {
        // This script contains a single Opcode that returns immediately (RET)
        // since all this transaction does is move Inputs and Outputs around.
        let script = Opcode::RET(REG_ONE).to_bytes().to_vec();
        Transaction::Script {
            gas_price: params.gas_price,
            gas_limit: params.gas_limit,
            byte_price: params.byte_price,
            maturity: params.maturity,
            receipts_root: Default::default(),
            script,
            script_data: vec![],
            inputs: inputs.to_vec(),
            outputs: outputs.to_vec(),
            witnesses: vec![],
            metadata: None,
        }
    }

    /// Get the balance of all spendable coins `asset_id` for address `address`. This is different
    /// from getting coins because we are just returning a number (the sum of UTXOs) instead of the
    /// UTXOs.
    pub async fn get_asset_balance(
        &self,
        address: &Address,
        asset_id: AssetId,
    ) -> std::io::Result<u64> {
        self.client
            .balance(&*address.to_string(), Some(&*asset_id.to_string()))
            .await
    }

    /// Get all the balances of all assets for address `address`. This is different from getting
    /// the coins because we are only returning the sum of UTXOs and not the UTXOs themselves
    pub async fn get_balances(
        &self,
        address: &Address,
    ) -> io::Result<PaginatedResult<schema::balance::Balance, String>> {
        let pagination = PaginationRequest {
            cursor: None,
            results: 9999,
            direction: PageDirection::Forward,
        };
        self.client
            .balances(&*address.to_string(), pagination)
            .await
    }

    /// Get transaction by id.
    pub async fn get_transaction_by_id(&self, tx_id: &str) -> io::Result<TransactionResponse> {
        Ok(self.client.transaction(tx_id).await.unwrap().unwrap())
    }

    // @todo
    // - Get transaction(s)
    // - Get block(s)
}

#[cfg(test)]
mod tests {
    use crate::LocalWallet;
    use fuels::prelude::{setup_address_and_coins, setup_test_provider};
    #[tokio::test]
    async fn test_balance() {
        let (private_key, coins) = setup_address_and_coins(10, 11);
        let (provider, _) = setup_test_provider(coins).await;
        let wallet = LocalWallet::new_from_private_key(private_key, provider);
        for (_utxo_id, coin) in coins {
            let balance = provider
                .get_asset_balance(&wallet.address, coin.asset_id)
                .await;
            assert_eq!(balance.unwrap(), 10);
        }
    }
}
