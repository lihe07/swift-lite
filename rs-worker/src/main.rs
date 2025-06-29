mod detect;
mod net;
mod nms;

use detect::Detector;

#[tokio::main]
async fn main() -> ! {
    loop {
        if let Err(e) = net::main_loop().await {
            println!("Error in main loop: {:?}", e);
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
