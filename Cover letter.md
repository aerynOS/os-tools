## Problem we're solving

Currently, Boulder would first copy Git repos or tarballs into `%(sourcedir)`, and then extract (if `unpack: true`) or copy the repo
into the build directory. This is of course inefficient, although it makes the container setup easier, because there is only one
directory (`%(sourcedir)`) to mount.

## Solution

This MR immediately sets up the bind-mounted paths to point to Git repositories or tarballs, or immediately extract tarballs outside
of the container with `bsdtar`, making builds more efficient.

When it comes to Git repositories, worktrees are used. `.git` for worktree is a text file that contains the path of the parent `.git` directory.
Since such directory must also exists, it gets bind-mounted in R/O mode under the `.git-parent`. name, e.g.:

```text
workdir
  |-- .git
  |-- .git-parent
  |-- source1.rs
  |-- source2.rs
  |-- README.md
```

Of course this mandates the existence of `bsdtar` and `git` on the host system,
but it shouldn't be a problem because `boulder recipe new` already expects that.

## Breaking changes

There is a breaking change introduced with this approach: `%(sourcedir)` is no longer necessary. I don't see it as a problem honestly,
because everything can just be extracted/bind-mounted into `%(workdir)`. `%(sourcedir)` was basically a mere intermediate path before
actual copy/extraction occurred. Anyway, this is a breaking change in that many recipes rely on `%(sourcedir)`, so they must be updated.
It's not a big change, but it's worth remembering that.

## What about the "Prepare" phase?

The prepare phase was a script run inside the container that copied source into `%(workdir). This is no longer necessary, and in fact
this MR sets the preparation script to the empty string. Should we use the "Prepare" phase for operations performed by `boulder`,
instead of ops performed by a script?

## Caveats and future improvements

When a recipe has multiple build targets (e.g. x86_64 and emul32), they share the same container. This approach has a few downsides:

- **Introduced with this PR**: We share the sources upfront for all targets. This means possibly extracting an archive twice.
If the first target fails, we've extracted the source for the second target pointlessly. Recipes with multiple targets are rare,
but still it's an inefficiency.\
The same applies to Git repositories: we create worktrees upfront.

- **Pre-existing behavior**: Targets share the same container. Since we run arbitrary code inside containers, a build script
could contaminate all targets.

I think we should eventually compartmentalize build targets.

## Test plan
