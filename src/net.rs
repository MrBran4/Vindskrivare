use core::str::from_utf8;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embassy_net::Stack;
use log::{error, info};
use reqwless::client::HttpClient;
use reqwless::request::Method;

#[embassy_executor::task]
pub async fn worker(stack: Stack<'static>) {
    loop {
        let mut rx_buffer = [0; 8192];

        let client_state = TcpClientState::<1, 1024, 1024>::new();
        let tcp_client = TcpClient::new(stack, &client_state);
        let dns_client = DnsSocket::new(stack);

        let mut http_client = HttpClient::new(&tcp_client, &dns_client);
        let url = "https://api.myip.com";

        info!("connecting to {}", &url);

        let mut request = match http_client.request(Method::GET, url).await {
            Ok(req) => req,
            Err(e) => {
                log::error!("Failed to make HTTP request: {:?}", e);
                return; // handle the error
            }
        };

        let response = match request.send(&mut rx_buffer).await {
            Ok(resp) => resp,
            Err(_e) => {
                error!("Failed to send HTTP request");
                return; // handle the error;
            }
        };

        let body = match from_utf8(response.body().read_to_end().await.unwrap()) {
            Ok(b) => b,
            Err(_e) => {
                error!("Failed to read response body");
                return; // handle the error
            }
        };
        info!("Response body: {:?}", &body);
    }
}
