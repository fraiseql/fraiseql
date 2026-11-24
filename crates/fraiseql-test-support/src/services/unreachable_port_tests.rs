//! The port the unreachable-URL tests name must stay unreachable while they run, whatever
//! other tests bind concurrently. A port freed after binding `127.0.0.1:0` does not: the next
//! ephemeral bind anywhere may take it, the probe then connects, and the test reads a
//! reachable service as unreachable's failure.

use super::unreachable_port;

#[test]
fn no_socket_can_listen_on_the_unreachable_port() {
    let port = unreachable_port();
    let listener = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
    assert_ne!(
        listener.local_addr().unwrap().port(),
        port,
        "another test could listen on the port the unreachable-URL tests probe"
    );
}

#[test]
fn the_unreachable_port_refuses_a_connection() {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], unreachable_port()));
    assert!(
        std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_secs(1)).is_err()
    );
}
