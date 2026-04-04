use super::*;
use oneshim_core::models::sync::ChangeSetKind;
use oneshim_core::sync::Hlc;

use super::super::lan_discovery::LanPeerInfo;

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

#[tokio::test]
async fn push_to_offline_peer_is_graceful() {
    let transport = LanSyncTransport::start(
        "dev-offline-push".to_string(),
        "Offline Push".to_string(),
        "pass".to_string(),
        b"cert".to_vec(),
        b"key".to_vec(),
        "fp".to_string(),
        0,
        false,
    )
    .await
    .unwrap();

    // Inject a ghost peer at a port where nobody is listening
    transport.verified_peers.write().insert(
        "ghost".to_string(),
        LanPeerInfo {
            device_id: "ghost".to_string(),
            device_name: "Ghost".to_string(),
            host: "127.0.0.1".to_string(),
            port: 19999,
            fingerprint: "fp-ghost".to_string(),
            version: "1".to_string(),
        },
    );

    // Push should succeed (best-effort fanout, does not fail overall)
    let cs = test_changeset();
    let result = transport.push(&cs).await;
    assert!(result.is_ok());

    transport.stop();
}

#[tokio::test]
async fn pull_from_offline_peer_returns_none() {
    let transport = LanSyncTransport::start(
        "dev-offline-pull".to_string(),
        "Offline Pull".to_string(),
        "pass".to_string(),
        b"cert".to_vec(),
        b"key".to_vec(),
        "fp".to_string(),
        0,
        false,
    )
    .await
    .unwrap();

    // Inject a ghost peer at a port where nobody is listening
    transport.verified_peers.write().insert(
        "ghost".to_string(),
        LanPeerInfo {
            device_id: "ghost".to_string(),
            device_name: "Ghost".to_string(),
            host: "127.0.0.1".to_string(),
            port: 19999,
            fingerprint: "fp-ghost".to_string(),
            version: "1".to_string(),
        },
    );

    // Pull from offline peer: Ok(None) or Err are both acceptable
    let result = transport.pull(&Hlc::default()).await;
    match result {
        Ok(None) => {} // graceful: no data from unreachable peer
        Err(_) => {}   // also acceptable: connection failed
        Ok(Some(_)) => panic!("should not get data from offline peer"),
    }

    transport.stop();
}

// -----------------------------------------------------------------------
// Task 7: Sync Verification Tests
// -----------------------------------------------------------------------

/// Helper to create a changeset with a specific origin and watermark.
fn changeset_with_watermark(origin: &str, wall_ms: u64, counter: u32) -> ChangeSet {
    ChangeSet {
        kind: ChangeSetKind::Data,
        origin_device_id: origin.to_string(),
        origin_device_name: format!("Device {origin}"),
        watermark: Hlc {
            wall_ms,
            counter,
            device_id: origin.to_string(),
        },
        segments: vec![serde_json::json!({"id": format!("seg-{origin}")})],
        ..Default::default()
    }
}

/// Helper to create a transport and inject a peer, returning (transport, peer_port).
async fn start_pair(
    id_a: &str,
    id_b: &str,
    passphrase: &str,
) -> (LanSyncTransport, LanSyncTransport) {
    let a = LanSyncTransport::start(
        id_a.to_string(),
        format!("Peer {id_a}"),
        passphrase.to_string(),
        b"cert".to_vec(),
        b"key".to_vec(),
        format!("fp-{id_a}"),
        0,
        false,
    )
    .await
    .unwrap();

    let b = LanSyncTransport::start(
        id_b.to_string(),
        format!("Peer {id_b}"),
        passphrase.to_string(),
        b"cert".to_vec(),
        b"key".to_vec(),
        format!("fp-{id_b}"),
        0,
        false,
    )
    .await
    .unwrap();

    let a_port = a.server_port();
    let b_port = b.server_port();

    // A knows about B
    a.verified_peers.write().insert(
        id_b.to_string(),
        LanPeerInfo {
            device_id: id_b.to_string(),
            device_name: format!("Peer {id_b}"),
            host: "127.0.0.1".to_string(),
            port: b_port,
            fingerprint: format!("fp-{id_b}"),
            version: "1".to_string(),
        },
    );

    // B knows about A
    b.verified_peers.write().insert(
        id_a.to_string(),
        LanPeerInfo {
            device_id: id_a.to_string(),
            device_name: format!("Peer {id_a}"),
            host: "127.0.0.1".to_string(),
            port: a_port,
            fingerprint: format!("fp-{id_a}"),
            version: "1".to_string(),
        },
    );

    // Yield to let both servers start accepting connections
    tokio::task::yield_now().await;

    (a, b)
}

#[tokio::test]
async fn bidirectional_sync_roundtrip() {
    let (a, b) = start_pair("bi-a", "bi-b", "bidir-pass").await;

    // A pushes a changeset to B
    let cs_a = changeset_with_watermark("bi-a", 1000, 1);
    a.push(&cs_a).await.unwrap();

    // B should have received A's data
    let received_by_b = b.drain_received();
    assert_eq!(
        received_by_b.len(),
        1,
        "B should receive 1 changeset from A"
    );
    assert_eq!(received_by_b[0].origin_device_id, "bi-a");

    // B pushes a different changeset to A
    let cs_b = changeset_with_watermark("bi-b", 2000, 1);
    b.push(&cs_b).await.unwrap();

    // A should have received B's data
    let received_by_a = a.drain_received();
    assert_eq!(
        received_by_a.len(),
        1,
        "A should receive 1 changeset from B"
    );
    assert_eq!(received_by_a[0].origin_device_id, "bi-b");

    a.stop();
    b.stop();
}

#[tokio::test]
async fn watermark_filtering_skips_old_data() {
    let passphrase = "watermark-test-pass";

    // Provider enqueues a changeset at T=1000
    let provider = LanSyncTransport::start(
        "wm-provider".to_string(),
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

    let cs = changeset_with_watermark("wm-provider", 1000, 1);
    provider.enqueue_outbound(cs);
    let provider_port = provider.server_port();

    // Consumer connects to the provider
    let consumer = LanSyncTransport::start(
        "wm-consumer".to_string(),
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
        "wm-provider".to_string(),
        LanPeerInfo {
            device_id: "wm-provider".to_string(),
            device_name: "Provider".to_string(),
            host: "127.0.0.1".to_string(),
            port: provider_port,
            fingerprint: "fp-prov".to_string(),
            version: "1".to_string(),
        },
    );

    tokio::task::yield_now().await;

    // Pull with since before T=1000 should return the changeset
    let since_before = Hlc {
        wall_ms: 500,
        counter: 0,
        device_id: String::new(),
    };
    let pulled = consumer.pull(&since_before).await.unwrap();
    assert!(
        pulled.is_some(),
        "pull with since before the watermark should return data"
    );
    assert_eq!(pulled.unwrap().origin_device_id, "wm-provider");

    // Pull with since after T=1000 should return no data (204)
    let since_after = Hlc {
        wall_ms: 1001,
        counter: 0,
        device_id: String::new(),
    };
    let pulled_none = consumer.pull(&since_after).await.unwrap();
    assert!(
        pulled_none.is_none(),
        "pull with since after the watermark should return no data"
    );

    provider.stop();
    consumer.stop();
}

#[tokio::test]
async fn concurrent_push_pull_no_data_loss() {
    let (a, b) = start_pair("conc-a", "conc-b", "concurrent-pass").await;

    // A and B push simultaneously
    let cs_a = changeset_with_watermark("conc-a", 3000, 1);
    let cs_b = changeset_with_watermark("conc-b", 3000, 2);

    let (res_a, res_b) = tokio::join!(a.push(&cs_a), b.push(&cs_b));
    res_a.unwrap();
    res_b.unwrap();

    // Both should have received each other's data
    let received_by_a = a.drain_received();
    let received_by_b = b.drain_received();

    assert_eq!(
        received_by_a.len(),
        1,
        "A should receive 1 changeset from B"
    );
    assert_eq!(received_by_a[0].origin_device_id, "conc-b");

    assert_eq!(
        received_by_b.len(),
        1,
        "B should receive 1 changeset from A"
    );
    assert_eq!(received_by_b[0].origin_device_id, "conc-a");

    a.stop();
    b.stop();
}
