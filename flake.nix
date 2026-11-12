{
  description = "ruststeve: the dev shell (Java 25 for datagen's server reports and JNI probe)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    { nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        # Minecraft 26.1.2 runs on Java 25 (its version.json says java_version 25).
        jdk = pkgs.jdk25;
      in
      {
        # Rust itself comes from rustup, which reads rust-toolchain.toml.
        devShells.default = pkgs.mkShell {
          packages = [
            jdk
            pkgs.rustup
          ];
          # datagen runs `java` from PATH; the jni crate finds libjvm through JAVA_HOME.
          JAVA_HOME = "${jdk.home}";
        };
      }
    );
}
