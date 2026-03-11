mod detect;
mod net;
mod nms;

use detect::Detector;

#[tokio::main]
async fn main() -> ! {
    // name is the second argument
    let args: Vec<String> = std::env::args().collect();
    let name = args.get(1).cloned().unwrap_or("worker".to_string());

    loop {
        if let Err(e) = net::main_loop(&name).await {
            println!("Error in main loop: {:?}", e);
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
