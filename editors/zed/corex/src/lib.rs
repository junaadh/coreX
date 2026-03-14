use zed_extension_api as zed;

struct CoreXExt;

impl zed::Extension for CoreXExt {
    fn new() -> Self
    where
        Self: Sized,
    {
        Self
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed_extension_api::LanguageServerId,
        _worktree: &zed_extension_api::Worktree,
    ) -> zed_extension_api::Result<zed_extension_api::Command> {
        if language_server_id.as_ref() == "corex-lsp" {
            Ok(zed::Command {
                command:
                    "/Users/junaadh/Developer/rust/core_x/target/debug/cxc"
                        .into(),
                args: vec!["lsp".into()],
                env: vec![],
            })
        } else {
            Err(format!(
                "unknown language server: {}",
                language_server_id.as_ref()
            ))
        }
    }
}

zed::register_extension!(CoreXExt);
