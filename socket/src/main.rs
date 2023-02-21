use std::{collections::HashMap, hash::Hash, sync::Arc, time::Duration};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpSocket, TcpStream},
    sync::mpsc::Sender,
};

struct RoomManager {
    rooms: HashMap<String, Room>,
    clients: HashMap<(String, usize), tokio::sync::mpsc::Sender<RoomMessage>>,
}
struct Room {
    cnt: usize,
    startNum: usize,
    endNum : usize,
}
#[derive(Debug)]
enum RoomMessage {
    Join(String, tokio::sync::mpsc::Sender<RoomMessage>),
    ClientID(usize),
    //room full
    Rejected(String,usize),
    // Leave,
    End(),
    Message(String),
    // ChangeManager(tokio::sync::mpsc::Sender<RoomMessage>)
}

fn main() {
    let mut rt = tokio::runtime::Builder::new_multi_thread();
    rt.enable_all();

    if let Ok(runtime) = rt.build() {
        runtime.block_on(initialize()); // start the server, blick_on is a blocking call
    }
}
async fn initialize() {
    let (manager_tx, mut manager_rx) = tokio::sync::mpsc::channel(1024);
    // let (client_tx, client_rx) = tokio::sync::mpsc::channel(1024);

    let mut manager = RoomManager {
        rooms: HashMap::with_capacity(16),
        clients: HashMap::with_capacity(204800),
    };

    tokio::spawn(listen(manager_tx));
    loop {
        if let Some(msg) = manager_rx.recv().await {
            match msg {
                RoomMessage::Join(room_id, tx) => {
                    let mut client_id = 0;
                    let mut authCode: String = String::from("asdf");
                    if let Some(r) = manager.rooms.get_mut(&room_id) {
                        r.cnt += 1;
                        client_id = r.cnt;
                        if client_id > r.endNum{
                            tx.send(RoomMessage::Rejected(authCode,client_id.wrapping_sub(r.endNum))).await;
                            manager.clients.insert((room_id, client_id), tx);
                            continue;
                        }
                    } else {
                        manager.rooms.insert(room_id.clone(), Room { cnt: 1, startNum: 1, endNum: 3 });
                        client_id = 1;
                    }
                    tx.send(RoomMessage::ClientID(client_id)).await;
                    manager.clients.insert((room_id, client_id), tx);
                }
                _ => {}
            }
        }
    }
}

async fn listen(tx: Sender<RoomMessage>) {
    let listener = tokio::net::TcpListener::bind("0.0.0.0:33889").await;

    match listener {
        Ok(s) => loop {
            if let Ok((socket, addr)) = s.accept().await {
                tokio::spawn(handle_conn(socket, addr, tx.clone()));
            }
        },
        Err(e) => {
            println!("Error: {}", e);
        }
    }
}

async fn handle_conn(mut sock: TcpStream, addr: std::net::SocketAddr, tx: Sender<RoomMessage>) {
    let (client_tx, mut client_rx) = tokio::sync::mpsc::channel(1024);

    let mut buf = [0; 1024];
    if let Ok(n) = sock.read(&mut buf).await {
        if n == 0 {
            return;
        }
        let s = String::from_utf8_lossy(&buf[..n]);

        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut req = httparse::Request::new(&mut headers);
        let res = req.parse(&buf).unwrap();
        if !res.is_partial() {
            match req.path {
                Some(ref p) => {
                    println!("join request");
                    tx.send(RoomMessage::Join("join".to_string(), client_tx)).await;
                }
                _ => {}
            }
            for h in req.headers {
                if h.name == "Cookie" {
                    println!("cookie : {:?}", String::from_utf8(h.value.to_vec()));
                }
            }
        }
    }
    for _ in 0..1 {
        let rcv = client_rx.recv().await;
        println!("recv {:?}", rcv);

        match rcv {
            Some(RoomMessage::ClientID(id)) => {
                sock.write_all(format!("HTTP/1.1 200 OK\r\nSet-Cookie: bb\r\n\r\nConnection id: {}", id).as_bytes())
                    .await;
            }
            Some(RoomMessage::Message(msg)) => {
                sock.write_all(format!("\ndata: {}", msg).as_bytes()).await;
            }
            Some(RoomMessage::Rejected(authCode,left)) => {
                sock.write_all(format!(
                    "HTTP/1.1 200 OK\r\nSet-Cookie: bb\r\n\r\n: waiting number{} authCode:{}", 
                    left,authCode).as_bytes()).await;
            }
            _ => {}
        }
    }
}