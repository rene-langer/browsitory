### Pull requests section

- Changed: a pull request card shows "Connect to GitHub…" (or Bitbucket) until you connect; a
  dialog asks for the account and the access token and says why if saving fails. The token
  form with Save, Forget and Cancel is gone from the card (#1012).
- Changed: a connected card lists its pull requests by itself when you open it, without a List
  button. "New pull request…" opens a dialog that keeps what you typed and says why if creating
  fails; the card's "…" menu has Refresh and Disconnect (#1012).
- Changed: each pull request reads "#7 Title" over its branches, author and state, with a
  button to open it in your browser in place of its full address (#1012).
- Fixed: a card's title, such as "GitHub: acme/widget (origin)", is no longer shown in
  uppercase (#1012).
