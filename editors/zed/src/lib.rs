// SPDX-License-Identifier: GPL-3.0-only

//! Zed extension running craft-tidy's language server, `craft-tidy lsp`.

use zed_extension_api::{self as zed, LanguageServerId, Result, Worktree, settings::LspSettings};

struct CraftTidy;

impl zed::Extension for CraftTidy {
    fn new() -> Self {
        CraftTidy
    }

    fn language_server_command(
        &mut self,
        id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<zed::Command> {
        let binary = LspSettings::for_worktree(id.as_ref(), worktree)
            .ok()
            .and_then(|settings| settings.binary);
        let command = match binary.as_ref().and_then(|binary| binary.path.clone()) {
            Some(path) => path,
            None => worktree.which("craft-tidy").ok_or(
                "craft-tidy was not found on PATH; install it with \
                 `sudo snap install craft-tidy --edge`, or set \
                 lsp.craft-tidy.binary.path",
            )?,
        };
        let args = binary
            .as_ref()
            .and_then(|binary| binary.arguments.clone())
            .unwrap_or_else(|| vec!["lsp".into()]);
        let env = binary
            .and_then(|binary| binary.env)
            .map(|env| env.into_iter().collect())
            .unwrap_or_default();
        Ok(zed::Command { command, args, env })
    }
}

zed::register_extension!(CraftTidy);
