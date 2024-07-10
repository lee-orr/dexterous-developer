mod cli_test {
    use dexterous_developer_test_utils::{
        recv_exit, recv_std, replace_library, setup_test, InMessage,
    };

    use tracing_test::traced_test;

    #[traced_test]
    #[tokio::test]
    async fn simple_cli_can_run() {
        let (_comms, send, mut output, _) = setup_test("simple_cli").await;

        recv_std(&mut output, "Hey!")
            .await
            .expect("Failed first line");
        let _ = send.send(InMessage::Std("\n".to_string()));
        recv_std(&mut output, "Hey!")
            .await
            .expect("Failed Second Line");
        let _ = send.send(InMessage::Std("exit\n".to_string()));
        recv_exit(&mut output, Some(0))
            .await
            .expect("Wrong Exit Code");
    }

    #[traced_test]
    #[tokio::test]
    async fn can_swap_a_system() {
        let (mut comms, send, mut output, _) = setup_test("simple_cli").await;

        recv_std(&mut output, "Hey!")
            .await
            .expect("Failed first line");
        replace_library("simple_system_swap", &mut comms, &mut output, &send).await;
        recv_std(&mut output, "Swapped Update System!")
            .await
            .expect("Failed Swapped Line");
        let _ = send.send(InMessage::Std("exit\n".to_string()));
        recv_exit(&mut output, Some(0))
            .await
            .expect("Wrong Exit Code");
    }
}
