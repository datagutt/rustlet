//! `rustlet completion <shell>` — prints a shell completion script to
//! stdout. Uses clap_complete's built-in generators; each shell's install
//! path differs so we leave those instructions in the subcommand's help.
//!
//! With `--dynamic`, a small shell snippet is appended after the
//! clap_complete script that queries the configured tronbyt for live
//! device ids (via `rustlet devices --ids-only`) when completing the
//! first positional of `push`, `delete`, and `list`. bash/zsh/fish all
//! get targeted snippets; other shells fall back to the static script.

use std::io;

use anyhow::Result;
use clap::CommandFactory;
use clap_complete::{generate, Shell};

use crate::Cli;

const BASH_DYNAMIC_SUFFIX: &str = r#"
# --- rustlet dynamic device id completion ---
_rustlet_devices_ids() {
    local ids
    ids=$(rustlet devices --ids-only 2>/dev/null)
    COMPREPLY=( $(compgen -W "${ids}" -- "${cur}") )
}
_rustlet_installations_ids() {
    local device="$1"
    local ids
    ids=$(rustlet list "$device" --ids-only 2>/dev/null)
    COMPREPLY=( $(compgen -W "${ids}" -- "${cur}") )
}
_rustlet_dynamic_wrapper() {
    _rustlet "$@"
    local cur="${COMP_WORDS[COMP_CWORD]}"
    local sub="${COMP_WORDS[1]}"
    case "${sub}" in
        push|delete)
            if [ "${COMP_CWORD}" = "2" ]; then
                _rustlet_devices_ids
            elif [ "${sub}" = "delete" ] && [ "${COMP_CWORD}" = "3" ]; then
                _rustlet_installations_ids "${COMP_WORDS[2]}"
            fi
            ;;
        list)
            if [ "${COMP_CWORD}" = "2" ]; then
                _rustlet_devices_ids
            fi
            ;;
    esac
}
complete -F _rustlet_dynamic_wrapper -o nosort -o bashdefault -o default rustlet
"#;

const ZSH_DYNAMIC_SUFFIX: &str = r#"
# --- rustlet dynamic device id completion ---
_rustlet_dynamic_devices() {
    local -a ids
    ids=(${(f)"$(rustlet devices --ids-only 2>/dev/null)"})
    _describe 'device' ids
}
_rustlet_dynamic_installations() {
    local device="${words[3]}"
    local -a ids
    ids=(${(f)"$(rustlet list "$device" --ids-only 2>/dev/null)"})
    _describe 'installation' ids
}
compdef '_arguments "2: :_rustlet_dynamic_devices"' 'rustlet push'
compdef '_arguments "2: :_rustlet_dynamic_devices" "3: :_rustlet_dynamic_installations"' 'rustlet delete'
compdef '_arguments "2: :_rustlet_dynamic_devices"' 'rustlet list'
"#;

const FISH_DYNAMIC_SUFFIX: &str = r#"
# --- rustlet dynamic device id completion ---
function __rustlet_devices
    rustlet devices --ids-only 2>/dev/null
end
function __rustlet_installations
    set -l device (commandline -opc)[3]
    test -n "$device"; and rustlet list "$device" --ids-only 2>/dev/null
end
complete -c rustlet -n '__fish_seen_subcommand_from push;   and not __fish_seen_subcommand_from (__rustlet_devices)' -f -a '(__rustlet_devices)'
complete -c rustlet -n '__fish_seen_subcommand_from delete; and not __fish_seen_subcommand_from (__rustlet_devices)' -f -a '(__rustlet_devices)'
complete -c rustlet -n '__fish_seen_subcommand_from list;   and not __fish_seen_subcommand_from (__rustlet_devices)' -f -a '(__rustlet_devices)'
"#;

pub fn run(shell: Shell, dynamic: bool) -> Result<()> {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut io::stdout());

    if dynamic {
        let suffix = match shell {
            Shell::Bash => Some(BASH_DYNAMIC_SUFFIX),
            Shell::Zsh => Some(ZSH_DYNAMIC_SUFFIX),
            Shell::Fish => Some(FISH_DYNAMIC_SUFFIX),
            _ => None,
        };
        if let Some(s) = suffix {
            print!("{s}");
        } else {
            eprintln!(
                "# rustlet: --dynamic is not supported for {shell:?}; only static completions emitted"
            );
        }
    }
    Ok(())
}
