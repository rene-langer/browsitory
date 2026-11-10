### Patches and symbolic links

- Fixed: applying a patch, or applying or reverting a commit's file or hunk, failed whenever it
  changed, deleted or renamed a symbolic link. The link itself is now changed, as `git apply`
  does, and a patch series that fails later puts such a link back as it was (#563).
