# nix-cleaner

A GTK4 application for inspecting what occupies the Nix store, which garbage
collection roots keep it alive, and what removing a root would save.

## Panels

- **Store** groups store paths by package name, with duplicate counts, item
  sizes, and the GC roots that reference each group.
- **Roots** lists every discovered profile, generation, and result link, shows
  its protection status and solo savings, and lets you select roots to clean.
- **Tree** shows the closure of a selected root as a dependency tree, with both
  the size including dependencies and the path's own size.

## Safety model

Roots that keep the running system, the booted system, the active generation,
the newest three generations, or an active profile alive are marked protected
and cannot be selected. The GUI never modifies roots itself: it writes a plan
file and asks the privileged helper to verify it against a fresh root scan.

The helper only accepts links below `/nix/var/nix/profiles/`,
`/nix/var/nix/gcroots/per-user/`, or the plan's user home, and refuses
`/nix/var/nix/profiles/system`, `/run/`, and `/etc/` outright. It stages roots by
renaming them to `.nix-cleaner-staging` before running garbage collection, so a
dry run can report dead paths and bytes and then restore every link. Staging
also makes `nix-store --gc --print-dead` consider automatic GC roots stale, so
the helper snapshots and restores those links as well. Cleaning is irreversible
only after collection succeeds.

## Running from the devshell

```
nix develop
cargo run -p nix-cleaner
```

The helper builds next to the application, so the GUI finds
`target/debug/nix-cleaner-helper` automatically. Dry runs also go through
`pkexec`.

## Cleaning

Cleaning is only available through the installed polkit action
`dev.den.nix-cleaner.clean`. Enable it with `programs.nix-cleaner.enable = true`
and import `modules/nixos.nix`, which installs the package and the action file.
The helper is invoked through `pkexec`, never `sudo`.

## Size model

Closures follow the same rules as the Nix garbage collector, including
`keep-derivations` and `keep-outputs` read from `nix config show`. With
`keep-derivations = true`, the derivation of every live output is live too, and
its inputs come along, which is why derivation paths dominate a closure.

`In vacuum` is the closure size of one root alone. `Solo savings` is the part of
that closure no other root reaches. Selection savings is the sum of paths whose
every owner is in the selection, so shared dependencies are only counted once.

## Known limitations

- Sizes are `narSize` from the Nix database, not disk blocks. The optional
  `Measure on disk` button walks one closure and reports block usage plus the
  part shared through hardlinks.
- `df` on the store filesystem is only comparable when the store has its own
  filesystem. The Validate dialog prints it next to the computed numbers rather
  than drawing a conclusion.
- A small drift against `nix-store --gc --print-dead` is expected: temporary
  roots under `/nix/var/nix/temproots` and store paths held open by running
  processes are not part of the scan.
- Derivation paths are listed like any other store path, which makes the store
  panel noisy; filtering them is a deliberate open decision.
