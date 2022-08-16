// #![allow(unused)]
use std::thread;
use std::time::Duration;
use tonic::transport::Channel;
use tonic::{metadata::MetadataValue, Request};
use tower::timeout::Timeout;

use stat_common::server_status::server_status_client::ServerStatusClient;
use stat_common::server_status::StatRequest;

use crate::sample_all;
use crate::Args;
use crate::INTERVAL_MS;

pub async fn report(args: &Args, stat_base: &mut StatRequest) -> anyhow::Result<()> {
    let auth_user: String = args.user.to_string();
    let ssr_auth: &[u8] = b"single";

    let token = MetadataValue::try_from(format!("{}@_@{}", auth_user, args.pass))?;

    let channel = Channel::from_shared(args.addr.to_string())?.connect().await?;
    let timeout_channel = Timeout::new(channel, Duration::from_millis(5000));

    let grpc_client = ServerStatusClient::with_interceptor(timeout_channel, move |mut req: Request<()>| {
        req.metadata_mut().insert("authorization", token.clone());
        req.metadata_mut()
            .insert("ssr-auth", MetadataValue::try_from(ssr_auth).unwrap());

        Ok(req)
    });

    loop {
        let stat_rt = sample_all(args, stat_base);
        let mut client = grpc_client.clone();
        tokio::spawn(async move {
            let request = Request::new(stat_rt);

            match client.report(request).await {
                Ok(resp) => {
                    info!("grpc report resp => {:?}", resp);
                }
                Err(status) => {
                    error!("grpc report status => {:?}", status);
                }
            }
        });

        thread::sleep(Duration::from_millis(INTERVAL_MS));
    }
}
