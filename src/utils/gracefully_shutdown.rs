use tokio::signal;
use tokio_util::sync::CancellationToken;

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {tracing::debug!("Signal received, starting graceful shutdown");},
        _ = terminate => {tracing::debug!("Signal received, starting graceful shutdown");},
    }
}

pub fn shutdown_token() -> CancellationToken {
    let token = CancellationToken::new();
    let t = token.clone();

    tokio::spawn(async move {
        shutdown_signal().await; // 复用已有的函数
        t.cancel();
    });

    token
}
