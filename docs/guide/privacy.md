# Privacy

Browsitory collects no data about you. It has no telemetry, no analytics, no crash reports and
no account. It doesn't send us your repositories, your file names or how you use the app.

This page lists every connection Browsitory makes, and everything it keeps on your computer, so
you can check that claim. It covers the desktop app, the VSCode extension and this user guide
site. It is meant to meet the EU General Data Protection Regulation (GDPR) and similar laws,
such as the UK GDPR, the Swiss FADP and the CCPA.

!!! note "Who is responsible"
    Browsitory is an open-source project. Questions about this notice or your data:
    open an issue at <https://github.com/rene-langer/browsitory/issues>.
    Issues are public, so don't put personal data in one.

    <!-- TODO(owner): name the controller and add a private contact (e-mail or postal address)
    for privacy requests. -->

## In short

- Browsitory talks only to the servers you choose (your Git remotes and, if you set it up,
  GitHub's or Bitbucket's pull request API) and to GitHub, to look for updates of the desktop
  app.
- Everything else stays on your computer: your settings, your recent repositories, a log file
  and some state inside each repository's `.git` folder.
- Tokens and passwords go to your system's credential store, never into a file of ours.

## Connections Browsitory makes

Each connection below goes from your computer straight to the server named. Browsitory has no
server of its own, so we never see this traffic. Like any server, the one you connect to sees
your IP address and the time of the request; what else it learns is listed for each.

### Update check (desktop app only)

The desktop app checks for a new version when it starts, then every 6 hours while it runs.
When a check fails, it tries again after 10 minutes, then after longer waits up to 3 hours.

- **Where:** first `github.com` (the file `latest.json` of the latest Browsitory release),
  and, only when that fails, the same file on this site, `rene-langer.github.io`. Both are run
  by GitHub.
- **What they see:** your IP address, the time, and a request header naming the update
  library (`tauri-plugin-updater` and its version). The request says nothing about you or your
  repositories.
- **When there is an update:** the app downloads it from GitHub's release files and checks its
  signature. It installs it only when you choose **Restart to update**.
- **Turning it off:** not possible yet; a setting is planned
  ([#630](https://github.com/rene-langer/browsitory/issues/630)). Until then, block the two
  addresses above in your firewall if you don't want the check. The VSCode extension never
  checks for updates itself: your editor does that (see below).

GitHub's handling of this data: the
[GitHub General Privacy Statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement).
We see only how often each release file was downloaded, not by whom.

### Your Git remotes

Fetch, pull, push and clone connect to the remotes you added, over HTTPS or SSH, and send what
Git sends: the commits you push, and your token or SSH key to sign in. With **Fetch
automatically** turned on (off by default), Browsitory fetches the open repository's remotes
in the background at the interval you chose. No other server is involved. The server's owner
(for example GitHub, GitLab, Bitbucket or your company) handles that data under its own terms.

### Pull requests (only if you set them up)

When you save a personal access token under **Pull requests**, Browsitory asks the forge's API
for the repository's pull requests and creates the ones you ask for. It connects to
`api.github.com` for GitHub remotes and `api.bitbucket.org` for Bitbucket remotes. The request
carries your token, the repository's owner and name, and the pull request you create (title,
description, branches). Without a saved token, Browsitory doesn't contact these APIs.

### Links you open

**Open user guide** in the command palette, the user guide button in the release notes, and a
pull request's link open in your web browser. Your browser then connects to that site
(`rene-langer.github.io` for the user guide, your forge for a pull request), as with any link.

### Programs you set up

Browsitory runs some programs that you choose or that the repository asks for: Git hooks, your
editor, terminal, diff and merge tools, and GPG or SSH to sign commits. What those programs do
on the network is up to them.

## Data kept on your computer

None of this leaves your computer, and we can't see it.

| What | Where |
|---|---|
| Settings: recent and open repositories, workspaces, branch filters, merge choices, diff, commit message, tool and fetch preferences, the last version whose release notes you saw | `config.toml` in Browsitory's config folder (see below) |
| Display choices: theme, panel sizes, open and closed sections, recent commands, recent searches, recent commit messages, the per-repository **Fetch automatically** and **Prune** choices | The app's web storage (`localStorage`) |
| HTTPS tokens for Git remotes and pull request tokens | Your system's credential store: Keychain on macOS, Credential Manager on Windows, the Secret Service (for example GNOME Keyring or KWallet) on Linux, under the names `com.browsitory.git` and `com.browsitory.forge` |
| Undo and redo history, a paused rebase, cherry-pick or revert | Inside each repository: the folder `.git/browsitory/` and Git references under `refs/browsitory/` (in a linked worktree, in that worktree's own Git folder) |
| A log file of errors, with up to 5 older copies (desktop app) | `browsitory.log` in the app's log folder, see [Troubleshooting](troubleshooting.md) |

The log file holds errors and warnings. An error message can contain a file path, a branch
name or a remote's address. Browsitory writes no other log files.

The web storage holds the path of each repository it remembers something for, and the text of
your recent searches and commit messages.

### Deleting it all

Uninstall the app, then delete what you want to remove:

1. **Settings:** the config folder, `~/.config/browsitory` on Linux,
   `~/Library/Application Support/com.browsitory.Browsitory` on macOS,
   `%APPDATA%\browsitory\Browsitory` on Windows.
2. **Web storage, log files and downloaded updates (desktop app):** every folder named
   `com.browsitory.desktop` (or `com.browsitory.app`, from version 0.3.x and older) in
   `~/.local/share`, `~/.config` and `~/.cache` on Linux; in `~/Library/Application Support`,
   `~/Library/Caches`, `~/Library/Logs`, `~/Library/WebKit` and `~/Library/HTTPStorages` on
   macOS; in `%APPDATA%` and `%LOCALAPPDATA%` on Windows.
3. **Tokens:** before you uninstall, use **Forget HTTPS credential** in a remote's
   credentials dialog and **Forget token** under **Pull requests**. Or delete the entries named
   `com.browsitory.git` and `com.browsitory.forge` in your system's credential store.
4. **Per repository** (this also deletes undo history): delete `.git/browsitory/` and run
   `git for-each-ref --format="delete %(refname)" refs/browsitory/ | git update-ref --stdin`.

If you set `XDG_CONFIG_HOME`, `XDG_DATA_HOME` or `XDG_CACHE_HOME` on Linux, look there instead
of `~/.config`, `~/.local/share` and `~/.cache`.

## The VSCode extension

The extension works the same way, with these differences:

- It never checks for updates and makes no connection of its own beyond those above (your
  remotes, and the pull request APIs if you set them up). VSCode, or your editor, installs and
  updates it from the
  [Visual Studio Marketplace](https://marketplace.visualstudio.com/) or
  [Open VSX](https://open-vsx.org/), under their own privacy terms.
- It sends no telemetry, so VSCode's `telemetry.telemetryLevel` setting has nothing to turn off.
- It shares the settings file and the credential store with the desktop app.
- Its web storage, the last version whose release notes you saw, and the messages in its
  "Browsitory" output channel are kept by VSCode, in VSCode's own folders. The extension writes
  no log file of its own.

## This user guide site

The site is hosted on GitHub Pages, so GitHub sees your IP address and the pages you visit
([GitHub's privacy statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement)).
Each page also asks `api.github.com`, from your browser, for the repository's star count and
latest version, shown in the header. The site uses no cookies, no analytics and no web fonts
from other servers. Your browser keeps your light or dark choice and that header data in its
own storage for this site.

## Your rights

Because Browsitory collects no personal data, we hold none about you: there is nothing for us
to give you a copy of, correct or delete. You control everything Browsitory keeps on your
computer, and the steps above remove it.

For data held by GitHub, Bitbucket, the Marketplace, Open VSX or the host of your remotes,
contact that provider: they decide how it is processed. You can also complain to the data
protection authority where you live.

## Changes to this notice

This notice changes when Browsitory's behaviour does. The project's
[changelog](https://github.com/rene-langer/browsitory/blob/main/CHANGELOG.md) records each
change. A check in the project's CI fails when the code names a server that this page doesn't
list.

*Last updated: November 2026.*
