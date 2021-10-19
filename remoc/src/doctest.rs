use futures::Future;

#[cfg(feature = "rch")]
pub async fn loop_channel<T>() -> (
    (crate::rch::base::Sender<T>, crate::rch::base::Receiver<T>),
    (crate::rch::base::Sender<T>, crate::rch::base::Receiver<T>),
)
where
    T: crate::RemoteSend,
{
    use futures::StreamExt;

    let (transport_a_tx, transport_b_rx) = futures::channel::mpsc::channel::<bytes::Bytes>(0);
    let (transport_b_tx, transport_a_rx) = futures::channel::mpsc::channel::<bytes::Bytes>(0);

    let transport_a_rx = transport_a_rx.map(Ok::<_, std::io::Error>);
    let transport_b_rx = transport_b_rx.map(Ok::<_, std::io::Error>);

    let a = async move {
        let (conn, tx, rx) =
            crate::Connect::framed(Default::default(), transport_a_tx, transport_a_rx).await.unwrap();
        tokio::spawn(conn);
        (tx, rx)
    };

    let b = async move {
        let (conn, tx, rx) =
            crate::Connect::framed(Default::default(), transport_b_tx, transport_b_rx).await.unwrap();
        tokio::spawn(conn);
        (tx, rx)
    };

    futures::join!(a, b)
}

#[cfg(feature = "rch")]
pub async fn client_server<T, ClientFut, ServerFut>(
    client: impl FnOnce(crate::rch::base::Sender<T>) -> ClientFut,
    server: impl FnOnce(crate::rch::base::Receiver<T>) -> ServerFut,
) where
    T: crate::RemoteSend,
    ClientFut: Future<Output = ()> + Send + 'static,
    ServerFut: Future<Output = ()> + Send + 'static,
{
    let ((a_tx, _a_rx), (_b_tx, b_rx)) = loop_channel().await;
    futures::join!(client(a_tx), server(b_rx));
}
