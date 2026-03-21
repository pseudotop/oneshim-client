use super::*;

use super::*;
use oneshim_core::models::sync::ChangeSetKind;

fn test_changeset() -> ChangeSet {
    ChangeSet {
        kind: ChangeSetKind::Data,
        origin_device_id: "dev-a".to_string(),
        origin_device_name: "Test A".to_string(),
        watermark: Hlc {
            wall_ms: 100,
            counter: 1,
            device_id: "dev-a".to_string(),
        },
        segments: vec![serde_json::json!({"id": "seg-1"})],
        ..Default::default()
    }
}

#[tokio::test]
async fn transport_start_and_discover_empty() {
    let transport = LanSyncTransport::start(
        "dev-1".to_string(),
        "Test Mac".to_string(),
        "passphrase".to_string(),
        b"cert".to_vec(),
        b"key".to_vec(),
        "fp123".to_string(),
        0,
        false, // don't advertise in test
    )
    .await
    .unwrap();

    assert!(transport.server_port() > 0);

    let peers = transport.discover_peers().await.unwrap();
    assert!(peers.is_empty());

    transport.stop();
}

#[tokio::test]
async fn push_to_no_peers_is_noop() {
    let transport = LanSyncTransport::start(
        "dev-1".to_string(),
        "Test".to_string(),
        "pass".to_string(),
        b"cert".to_vec(),
        b"key".to_vec(),
        "fp".to_string(),
        0,
        false,
    )
    .await
    .unwrap();

    let cs = ChangeSet::default();
    let result = transport.push(&cs).await;
    assert!(result.is_ok());

    transport.stop();
}

#[tokio::test]
async fn pull_from_no_peers_returns_none() {
    let transport = LanSyncTransport::start(
        "dev-1".to_string(),
        "Test".to_string(),
        "pass".to_string(),
        b"cert".to_vec(),
        b"key".to_vec(),
        "fp".to_string(),
        0,
        false,
    )
    .await
    .unwrap();

    let result = transport.pull(&Hlc::default()).await.unwrap();
    assert!(result.is_none());

    transport.stop();
}

#[tokio::test]
async fn push_to_local_server_roundtrip() {
    // Start two transports: "sender" pushes to "receiver"'s server.
    let passphrase = "shared-secret-123";

    // Start receiver
    let receiver = LanSyncTransport::start(
        "receiver".to_string(),
        "Receiver".to_string(),
        passphrase.to_string(),
        b"cert".to_vec(),
        b"key".to_vec(),
        "fp-recv".to_string(),
        0,
        false,
    )
    .await
    .unwrap();

    let receiver_port = receiver.server_port();

    // Manually inject receiver as a verified peer in sender's peer map
    let sender = LanSyncTransport::start(
        "sender".to_string(),
        "Sender".to_string(),
        passphrase.to_string(),
        b"cert".to_vec(),
        b"key".to_vec(),
        "fp-send".to_string(),
        0,
        false,
    )
    .await
    .unwrap();

    // Inject receiver as a known peer
    sender.verified_peers.write().insert(
        "receiver".to_string(),
        LanPeerInfo {
            device_id: "receiver".to_string(),
            device_name: "Receiver".to_string(),
            host: "127.0.0.1".to_string(),
            port: receiver_port,
            fingerprint: "fp-recv".to_string(),
            version: "1".to_string(),
        },
    );

    // Yield to let the server task start accepting connections
    tokio::task::yield_now().await;

    // Push a changeset (sender will auto-authenticate with receiver)
    let cs = test_changeset();
    sender.push(&cs).await.unwrap();

    // Verify receiver got it
    let received = receiver.drain_received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].origin_device_id, "dev-a");

    sender.stop();
    receiver.stop();
}

#[tokio::test]
async fn pull_from_peer_server() {
    let passphrase = "pull-test-pass";

    // Start a server with an outbound changeset
    let provider = LanSyncTransport::start(
        "provider".to_string(),
        "Provider".to_string(),
        passphrase.to_string(),
        b"cert".to_vec(),
        b"key".to_vec(),
        "fp-prov".to_string(),
        0,
        false,
    )
    .await
    .unwrap();

    let provider_port = provider.server_port();
    provider.enqueue_outbound(test_changeset());

    // Start a consumer and inject provider as a peer
    let consumer = LanSyncTransport::start(
        "consumer".to_string(),
        "Consumer".to_string(),
        passphrase.to_string(),
        b"cert".to_vec(),
        b"key".to_vec(),
        "fp-cons".to_string(),
        0,
        false,
    )
    .await
    .unwrap();

    consumer.verified_peers.write().insert(
        "provider".to_string(),
        LanPeerInfo {
            device_id: "provider".to_string(),
            device_name: "Provider".to_string(),
            host: "127.0.0.1".to_string(),
            port: provider_port,
            fingerprint: "fp-prov".to_string(),
            version: "1".to_string(),
        },
    );

    // Yield to let the server task start accepting connections
    tokio::task::yield_now().await;

    // Pull from provider (consumer will auto-authenticate)
    let pulled = consumer.pull(&Hlc::default()).await.unwrap();

    assert!(pulled.is_some());
    let cs = pulled.unwrap();
    assert_eq!(cs.origin_device_id, "dev-a");
    assert_eq!(cs.segments.len(), 1);

    provider.stop();
    consumer.stop();
}

#[tokio::test]
async fn pull_wrong_passphrase_returns_none() {
    // Server uses one passphrase, client uses another
    let provider = LanSyncTransport::start(
        "provider".to_string(),
        "Provider".to_string(),
        "server-pass".to_string(),
        b"cert".to_vec(),
        b"key".to_vec(),
        "fp".to_string(),
        0,
        false,
    )
    .await
    .unwrap();

    provider.enqueue_outbound(test_changeset());
    let provider_port = provider.server_port();

    let consumer = LanSyncTransport::start(
        "consumer".to_string(),
        "Consumer".to_string(),
        "wrong-pass".to_string(),
        b"cert".to_vec(),
        b"key".to_vec(),
        "fp".to_string(),
        0,
        false,
    )
    .await
    .unwrap();

    consumer.verified_peers.write().insert(
        "provider".to_string(),
        LanPeerInfo {
            device_id: "provider".to_string(),
            device_name: "Provider".to_string(),
            host: "127.0.0.1".to_string(),
            port: provider_port,
            fingerprint: "fp".to_string(),
            version: "1".to_string(),
        },
    );

    tokio::task::yield_now().await;

    // Pull should fail auth and return None (graceful degradation)
    let pulled = consumer.pull(&Hlc::default()).await;
    match pulled {
        Ok(None) => {} // graceful: auth failed, no data
        Err(_) => {}   // also acceptable
        Ok(Some(_)) => panic!("should not succeed with wrong passphrase"),
    }

    provider.stop();
    consumer.stop();
}

#[tokio::test]
async fn token_cache_is_used() {
    let passphrase = "cache-test";

    let receiver = LanSyncTransport::start(
        "receiver".to_string(),
        "Receiver".to_string(),
        passphrase.to_string(),
        b"".to_vec(),
        b"".to_vec(),
        "fp".to_string(),
        0,
        false,
    )
    .await
    .unwrap();

    let receiver_port = receiver.server_port();

    let sender = LanSyncTransport::start(
        "sender".to_string(),
        "Sender".to_string(),
        passphrase.to_string(),
        b"".to_vec(),
        b"".to_vec(),
        "fp".to_string(),
        0,
        false,
    )
    .await
    .unwrap();

    sender.verified_peers.write().insert(
        "receiver".to_string(),
        LanPeerInfo {
            device_id: "receiver".to_string(),
            device_name: "Receiver".to_string(),
            host: "127.0.0.1".to_string(),
            port: receiver_port,
            fingerprint: "fp".to_string(),
            version: "1".to_string(),
        },
    );

    tokio::task::yield_now().await;

    // First push: authenticates fresh
    let cs1 = test_changeset();
    sender.push(&cs1).await.unwrap();
    assert_eq!(receiver.drain_received().len(), 1);

    // Verify token is cached
    assert!(sender.token_cache.get("receiver").is_some());

    // Second push: uses cached token (no re-auth)
    let cs2 = test_changeset();
    sender.push(&cs2).await.unwrap();
    assert_eq!(receiver.drain_received().len(), 1);

    sender.stop();
    receiver.stop();
}
