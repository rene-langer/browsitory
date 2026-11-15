### Comparing a tag

- Fixed: "Compare with current branch" on a tag that was deleted meanwhile said
  `No branch named "refs/tags/v1.0".`; it now says `No tag named "v1.0".`, and a tag on
  unrelated history is named `v1.0` instead of `refs/tags/v1.0` (#872).
