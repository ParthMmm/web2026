use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread,
};

use photo_upload_client::{ApiError, Client, ClientError, Kind};

const PHOTO_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

struct Request {
    head: String,
    body: Vec<u8>,
}

fn read_request(reader: &mut BufReader<TcpStream>) -> Option<Request> {
    let mut head = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        head.push_str(&line);
        if line == "\r\n" {
            break;
        }
    }
    let length = head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())?
        })
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(Request { head, body })
}

fn not_found() -> String {
    let body = format!(r#"{{"_tag":"PhotoNotFound","photoId":"{PHOTO_ID}"}}"#);
    format!(
        "HTTP/1.1 404 Not Found\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    )
}

fn expect_not_found(result: Result<impl std::fmt::Debug, ClientError>) {
    match result {
        Err(ClientError::Http {
            status: 404,
            error: Some(ApiError::PhotoNotFound { .. }),
        }) => {}
        other => panic!("expected a 404 PhotoNotFound, got {other:?}"),
    }
}

#[test]
fn a_reused_connection_that_closes_before_answering_is_retried_once_on_a_new_one() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    let (connections, counted) = mpsc::channel();
    thread::spawn(move || {
        for (index, stream) in listener.incoming().enumerate() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            connections.send(index).unwrap();
            read_request(&mut reader).unwrap();
            stream.write_all(not_found().as_bytes()).unwrap();
            if index == 0 {
                // The second request on this connection arrives, and the server closes without an answer.
                read_request(&mut reader).unwrap();
                drop(stream);
            }
        }
    });

    let client = Client::new(&base, "token").unwrap();
    expect_not_found(client.get(PHOTO_ID));
    expect_not_found(client.get(PHOTO_ID));
    assert_eq!(counted.try_iter().collect::<Vec<_>>(), [0, 1]);
}

#[test]
fn put_file_streams_the_file_with_its_length() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    let (received, request) = mpsc::channel();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        received.send(read_request(&mut reader).unwrap()).unwrap();
        stream
            .write_all(b"HTTP/1.1 204 No Content\r\n\r\n")
            .unwrap();
    });
    let bytes: Vec<u8> = (0..300_000).map(|index| (index % 251) as u8).collect();
    let path = std::env::temp_dir().join(format!("put-file-{}", std::process::id()));
    std::fs::write(&path, &bytes).unwrap();

    let client = Client::new(&base, "token").unwrap();
    client.put_file(PHOTO_ID, Kind::Original, &path).unwrap();
    std::fs::remove_file(&path).unwrap();

    let Request { head, body } = request.recv().unwrap();
    assert!(head.starts_with(&format!("PUT /v1/photos/{PHOTO_ID}/original ")));
    assert!(
        head.to_ascii_lowercase()
            .contains(&format!("content-length: {}\r\n", bytes.len())),
        "{head}"
    );
    assert!(!head.to_ascii_lowercase().contains("transfer-encoding"));
    assert_eq!(body, bytes);
}

#[test]
fn put_original_file_sends_the_signed_headers_to_storage_without_the_owner_token() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let base = format!("http://{address}/");
    let (received, requests) = mpsc::channel();
    let bytes: Vec<u8> = (0..300_000).map(|index| (index % 251) as u8).collect();
    let length = bytes.len();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let received = received.clone();
            // The API and storage clients each hold a connection open, so serve each on its own thread.
            thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                while let Some(request) = read_request(&mut reader) {
                    let answer = if request.head.starts_with("POST ") {
                        let body = format!(
                            r#"{{"expiresAt":"2026-11-14T00:15:00.000Z","headers":{{"content-length":"{length}","x-amz-checksum-sha256":"c2lnbmVk"}},"method":"PUT","url":"http://{address}/private/photos/{PHOTO_ID}/original-x?X-Amz-Signature=abc"}}"#
                        );
                        format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
                            body.len()
                        )
                    } else {
                        "HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n".to_owned()
                    };
                    received.send(request).unwrap();
                    stream.write_all(answer.as_bytes()).unwrap();
                }
            });
        }
    });
    let path = std::env::temp_dir().join(format!("put-original-{}", std::process::id()));
    std::fs::write(&path, &bytes).unwrap();

    let client = Client::new(&base, "owner-secret").unwrap();
    client.put_original_file(PHOTO_ID, &path).unwrap();
    std::fs::remove_file(&path).unwrap();

    let sign = requests.recv().unwrap();
    assert!(
        sign.head
            .starts_with(&format!("POST /v1/photos/{PHOTO_ID}/original/upload-url ")),
        "{}",
        sign.head
    );
    assert!(sign.head.contains("owner-secret"));
    let Request { head, body } = requests.recv().unwrap();
    let lower = head.to_ascii_lowercase();
    assert!(
        head.starts_with(&format!(
            "PUT /private/photos/{PHOTO_ID}/original-x?X-Amz-Signature=abc "
        )),
        "{head}"
    );
    assert!(
        lower.contains(&format!("content-length: {length}\r\n")),
        "{head}"
    );
    assert!(
        lower.contains("x-amz-checksum-sha256: c2lnbmvk\r\n"),
        "{head}"
    );
    assert!(!lower.contains("authorization"), "{head}");
    assert!(!head.contains("owner-secret"), "{head}");
    assert_eq!(body, bytes);
}

#[test]
fn a_storage_refusal_names_its_s3_code() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let base = format!("http://{address}/");
    thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                while let Some(request) = read_request(&mut reader) {
                    let (status, kind, body) = if request.head.starts_with("POST ") {
                        (
                            "200 OK",
                            "application/json",
                            format!(
                                r#"{{"expiresAt":"2026-11-14T00:15:00.000Z","headers":{{"x-amz-checksum-sha256":"c2lnbmVk"}},"method":"PUT","url":"http://{address}/private/k"}}"#
                            ),
                        )
                    } else {
                        (
                            "400 Bad Request",
                            "application/xml",
                            "<?xml version=\"1.0\"?><Error><Code>BadDigest</Code></Error>"
                                .to_owned(),
                        )
                    };
                    let answer = format!(
                        "HTTP/1.1 {status}\r\ncontent-type: {kind}\r\ncontent-length: {}\r\n\r\n{body}",
                        body.len()
                    );
                    stream.write_all(answer.as_bytes()).unwrap();
                }
            });
        }
    });

    let client = Client::new(&base, "token").unwrap();
    match client.put_original(PHOTO_ID, vec![1, 2, 3]) {
        Err(ClientError::Storage {
            status: 400,
            code: Some(code),
        }) if code == "BadDigest" => {}
        other => panic!("expected storage 400 BadDigest, got {other:?}"),
    }
}
