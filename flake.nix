{
  description = "ruststeve: the dev shell (JDK 25 for datagen and the server) and the git hooks";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    git-hooks-nix.url = "github:cachix/git-hooks.nix";
    git-hooks-nix.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs =
    inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [ inputs.git-hooks-nix.flakeModule ];
      systems = [
        "aarch64-darwin"
        "aarch64-linux"
        "x86_64-linux"
      ];

      perSystem =
        { config, pkgs, ... }:
        let
          # Minecraft 26.1.2 runs on Java 25 (its version.json says java_version 25).
          jdk = pkgs.jdk25;
        in
        {
          # The hooks run cargo, which needs the crate registry: not in the nix flake check
          # sandbox. CI runs the same checks on every push.
          pre-commit.check.enable = false;

          pre-commit.settings = {
            # prek: the Rust reimplementation of pre-commit, faster, same config.
            package = pkgs.prek;

            hooks = {
              # The Rust checks use cargo from rustup, which reads rust-toolchain.toml: the same
              # rustfmt and clippy CI uses (nixpkgs' versions could format differently).
              cargo-fmt = {
                enable = true;
                name = "cargo fmt";
                entry = "cargo fmt --all -- --check";
                files = "\\.rs$";
                pass_filenames = false;
              };
              cargo-clippy = {
                enable = true;
                name = "cargo clippy -D warnings";
                entry = "cargo clippy --workspace --all-targets --locked -- -D warnings";
                files = "(\\.rs|Cargo\\.toml|Cargo\\.lock|clippy\\.toml)$";
                pass_filenames = false;
              };
              brain-check = {
                enable = true;
                name = "brain-check (links, orphans, sourceRefs)";
                entry = "cargo run -q -p brain-check";
                # A note, or any file a note may cite, changed.
                files = "^(brain/|crates/|\\.github/|.*\\.md$|.*\\.toml$|flake\\.nix$)";
                pass_filenames = false;
              };
              commit-msg = {
                enable = true;
                name = "commit message (no AI attribution, subject <= 72)";
                entry = "cargo run -q -p xtask -- commit-msg";
                stages = [ "commit-msg" ];
              };

              nixfmt.enable = true;
              actionlint.enable = true;
              check-merge-conflicts.enable = true;
              ripsecrets.enable = true;
              check-added-large-files = {
                enable = true;
                # The generated tables are large by nature (blocks.rs about 700 KB).
                excludes = [ "^crates/data/src/generated/" ];
              };
            };
          };

          devShells.default = pkgs.mkShell {
            # Rust itself comes from rustup, which reads rust-toolchain.toml.
            packages = [
              jdk
              pkgs.rustup
              # `prek run --all-files` runs every hook by hand.
              config.pre-commit.settings.package
            ]
            ++ config.pre-commit.settings.enabledPackages;
            # datagen runs `java` from PATH; the jni crate finds libjvm through JAVA_HOME.
            JAVA_HOME = "${jdk.home}";
            # Installs the git hooks on entering the shell.
            shellHook = config.pre-commit.installationScript;
          };
        };
    };
}
