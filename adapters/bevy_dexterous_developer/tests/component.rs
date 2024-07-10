mod component_test {
    use dexterous_developer_test_utils::{
        recv_exit, recv_std, replace_library, setup_test, InMessage,
    };

    use tracing_test::traced_test;

    #[traced_test]
    #[tokio::test]
    async fn can_serialize_a_component() {
        let (mut comms, send, mut output, _) =
            setup_test("serde_serializable_component_start").await;

        recv_std(&mut output, "a - b")
            .await
            .expect("Failed first line");
        replace_library(
            "serde_serializable_component_end",
            &mut comms,
            &mut output,
            &send,
        )
        .await;
        recv_std(&mut output, "a_? - b_?")
            .await
            .expect("Failed Second Line");
        let _ = send.send(InMessage::Std("exit\n".to_string()));
        recv_exit(&mut output, Some(0))
            .await
            .expect("Wrong Exit Code");
    }

    #[traced_test]
    #[tokio::test]
    async fn can_replace_a_component() {
        let (mut comms, send, mut output, _) = setup_test("replacable_component_start").await;

        recv_std(&mut output, "a - b")
            .await
            .expect("Failed first line");
        replace_library("replacable_component_end", &mut comms, &mut output, &send).await;
        recv_std(&mut output, "a_? - b_?")
            .await
            .expect("Failed Second Line");
        let _ = send.send(InMessage::Std("exit\n".to_string()));
        recv_exit(&mut output, Some(0))
            .await
            .expect("Wrong Exit Code");
    }

    #[traced_test]
    #[tokio::test]
    async fn can_reset_setup() {
        let (mut comms, send, mut output, _) = setup_test("reset_component").await;

        recv_std(&mut output, "a").await.expect("Failed first line");
        let _ = send.send(InMessage::Std("\n".to_string()));
        recv_std(&mut output, "a - b")
            .await
            .expect("Failed second line");
        replace_library("reset_component", &mut comms, &mut output, &send).await;
        recv_std(&mut output, "a").await.expect("Failed first line");
        let _ = send.send(InMessage::Std("\n".to_string()));
        recv_std(&mut output, "a - b")
            .await
            .expect("Failed second line");
        let _ = send.send(InMessage::Std("exit\n".to_string()));
        recv_exit(&mut output, Some(0))
            .await
            .expect("Wrong Exit Code");
    }
}
