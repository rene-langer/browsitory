# Pull requests

For GitHub and Bitbucket remotes, open **Pull requests** in the sidebar. Each remote has its own
card, titled with the provider, the repository and the remote ("GitHub: acme/widget (origin)").

**Connecting.** Until you connect, a card offers only **Connect to GitHub…** (or **Connect to
Bitbucket…**). The dialog asks for your account name and a personal access token; Bitbucket needs
a repository or workspace access token, not an app password. If saving the token fails, the
dialog says why. The token goes to your system's credential store, and the account name is
remembered, so every card for that provider is connected from then on, in every repository
you open (see [Privacy](privacy.md#pull-requests-only-if-you-set-them-up) for what that sends).

**Pull requests.** A connected card lists the repository's pull requests by itself each time you
open it: the number and title, then the source and target branches, the author and the state.
Click a pull request's link button to open it in your browser. **New pull request…** opens a
dialog for the title, an optional description, the source branch (the current branch at first)
and the target branch (`main` or `master` at first). If creating it fails, the dialog says why
and keeps what you typed.

The card's **…** menu has **New pull request…**, **Refresh** (list again) and **Disconnect**,
which forgets the token and hides the list.

The section lists nothing until the repository has a GitHub or Bitbucket remote. Add one with
**Add remote…** in the **+** menu of the Branches section.
