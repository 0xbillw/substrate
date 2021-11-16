use std::sync::Arc;

use futures::{future, StreamExt};
use jsonrpsee::ws_server::SubscriptionSink;
use sc_client_api::BlockchainEvents;
use sp_blockchain::HeaderBackend;
use sp_runtime::{generic::BlockId, traits::Block as BlockT};

/// Helper to create subscriptions for `allHeads` and `newHeads`.
pub async fn subscribe_headers<Client, Block>(
	client: Arc<Client>,
	mut sink: SubscriptionSink,
	method: &str,
) where
	Block: BlockT + 'static,
	Client: HeaderBackend<Block> + BlockchainEvents<Block> + 'static,
{
	let hash = client.info().best_hash;
	let best_head = match client.header(BlockId::Hash(hash)) {
		Ok(head) => head,
		Err(e) => {
			log_err(method, e);
			return
		},
	};

	// NOTE(niklasad1): this will only fail when the subscriber is offline or serialize fails.
	if let Err(e) = sink.send(&best_head) {
		log_err(method, e);
		return
	};

	// NOTE: by the time we set up the stream there might be a new best block and so there is a risk
	// that the stream has a hole in it. The alternative would be to look up the best block *after*
	// we set up the stream and chain it to the stream. Consuming code would need to handle
	// duplicates at the beginning of the stream though.
	let stream = client.import_notification_stream();
	stream
		.take_while(|import| {
			future::ready(sink.send(&import.header).map_or_else(
				|e| {
					log_err(method, e);
					false
				},
				|_| true,
			))
		})
		.for_each(|_| future::ready(()))
		.await;
}

/// Helper to create subscriptions for `finalizedHeads`.
// NOTE(niklasad1): almost identical to `subscribe_headers` but requires different stream and
// finalized head
// (could work with generic stream and block_hash but would require cloning extra Arc's)
pub async fn subscribe_finalized_headers<Client, Block>(
	client: Arc<Client>,
	mut sink: SubscriptionSink,
	method: &str,
) where
	Block: BlockT + 'static,
	Client: HeaderBackend<Block> + BlockchainEvents<Block> + 'static,
{
	let hash = client.info().finalized_hash;
	let best_head = match client.header(BlockId::Hash(hash)) {
		Ok(head) => head,
		Err(err) => {
			log_err(method, err);
			return
		},
	};

	// NOTE(niklasad1): this will only fail when the subscriber is offline or serialize fails.
	if let Err(err) = sink.send(&best_head) {
		log_err(method, err);
		return
	};

	// NOTE: by the time we set up the stream there might be a new best block and so there is a risk
	// that the stream has a hole in it. The alternative would be to look up the best block *after*
	// we set up the stream and chain it to the stream. Consuming code would need to handle
	// duplicates at the beginning of the stream though.
	let stream = client.finality_notification_stream();
	stream
		.take_while(|import| {
			future::ready(sink.send(&import.header).map_or_else(
				|e| {
					log_err(method, e);
					false
				},
				|_| true,
			))
		})
		.for_each(|_| future::ready(()))
		.await;
}

fn log_err<E: std::fmt::Debug>(method: &str, err: E) {
	log::debug!("Could not send data to subscription: {} error: {:?}", method, err);
}
