### Copy from the context menus

- Added: a commit's menu ends with **Copy SHA**, **Copy short SHA**, **Copy subject**, **Copy
  author** (`Name <email>`) and **Copy date** (ISO 8601); a local or remote branch's menu has
  **Copy branch name** (`remote/branch` for a remote branch), a remote's menu **Copy fetch URL**
  (and **Copy push URL** when it differs), and a tag's menu, in the Tags panel and on its badge
  in the graph, **Copy tag name**. They sit before the destructive items and stay enabled while
  an operation runs (#614).
- Changed: the Edit remote dialog's copy buttons confirm with a notification too, and a
  clipboard failure there or on the commit details' **Copy** shows an error instead of failing
  silently (#614).
