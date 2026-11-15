### Notifications

- Fixed: after a fetch, a branch both ahead of and behind its upstream was reported as only
  behind. The notification now says "main and origin/main have diverged: 1 commit ahead, 1
  behind."; a branch only ahead says so and offers **Push** (#999).
- Changed: notifications and the desktop app's update notice share one place, the bottom-left
  corner, so they stack instead of covering each other, and none covers the **Commit** button
  (#1000).
- Changed: the operation log shows why an operation failed first, in the app's words, then git's
  own text, then the facts and **Run again**; **Copy details** copies them. Rows read "Fetch from
  origin" and "Push to origin" (#1022).
