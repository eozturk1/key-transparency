pub mod common; // Declare as `pub` to work around Cargo issue: https://github.com/rust-lang/rust/issues/46379

use bytes::Bytes;
use common::{
    broadcast_certificate, committee, delete_storage, notification, spawn_witnesses, votes,
};
use function_name::named;
use futures::future::try_join_all;
use messages::publish::{PublishCertificate, Root, SequenceNumber};
use messages::sync::{PublishCertificateQuery, State};
use messages::{IdPToWitnessMessage, WitnessToIdPMessage};
use network::reliable_sender::ReliableSender;

#[tokio::test]
#[named]
async fn state_query() {
    let base_port = 8_000;
    let committee = committee(base_port);
    let test_id = function_name!();

    // Spawn 4 witnesses.
    spawn_witnesses(&test_id, &committee);
    tokio::task::yield_now().await;

    // Broadcast a state query.
    let addresses = committee
        .witnesses_addresses()
        .into_iter()
        .map(|(_, address)| address)
        .collect();
    let message = IdPToWitnessMessage::StateQuery;
    let serialized = bincode::serialize(&message).unwrap();
    let bytes = Bytes::from(serialized);
    let mut sender = ReliableSender::new();
    let handles = sender.broadcast(addresses, bytes).await;

    // Make the expected state.
    let expected = State {
        root: Root::default(),
        sequence_number: SequenceNumber::default(),
        lock: None,
    };

    // Ensure the witnesses' replies are as expected.
    for reply in try_join_all(handles).await.unwrap() {
        match bincode::deserialize(&reply).unwrap() {
            WitnessToIdPMessage::State(Ok(state)) => assert_eq!(state, expected),
            _ => panic!("Unexpected protocol message"),
        }
    }

    // Delete the storage.
    delete_storage(&test_id);
}

#[tokio::test]
#[named]
async fn sync_request() {
    let base_port = 8_100;
    let committee = committee(base_port);
    let test_id = function_name!();

    // Spawn 4 witnesses.
    spawn_witnesses(&test_id, &committee);
    tokio::task::yield_now().await;

    // Broadcast a certificate.
    let notification = notification();
    let certificate = PublishCertificate {
        root: notification.root.clone(),
        sequence_number: notification.sequence_number,
        votes: votes()
            .into_iter()
            .map(|x| (x.author, x.signature))
            .collect(),
    };
    let handles = broadcast_certificate(certificate.clone(), &committee).await;
    let _ = try_join_all(handles).await.unwrap();

    // Broadcast a sync request.
    let request = PublishCertificateQuery {
        sequence_number: notification.sequence_number,
    };

    let addresses = committee
        .witnesses_addresses()
        .into_iter()
        .map(|(_, address)| address)
        .collect();
    let message = IdPToWitnessMessage::PublishCertificateQuery(request);
    let serialized = bincode::serialize(&message).unwrap();
    let bytes = Bytes::from(serialized);
    let mut sender = ReliableSender::new();
    let handles = sender.broadcast(addresses, bytes).await;

    // Ensure the witnesses' replies are as expected.
    for reply in try_join_all(handles).await.unwrap() {
        match bincode::deserialize(&reply).unwrap() {
            WitnessToIdPMessage::PublishCertificateResponse(received) => {
                match bincode::deserialize(&received).unwrap() {
                    IdPToWitnessMessage::PublishCertificate(cert) => {
                        assert_eq!(cert.root, certificate.root);
                        assert_eq!(cert.sequence_number, certificate.sequence_number);
                    }
                    _ => panic!("Unexpected response"),
                }
            }
            _ => panic!("Unexpected protocol message"),
        }
    }

    // Delete the storage.
    delete_storage(&test_id);
}
