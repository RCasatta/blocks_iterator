{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };
    crane = {
      url = "github:ipetkov/crane";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
  };
  outputs = { self, nixpkgs, flake-utils, rust-overlay, crane }:
    flake-utils.lib.eachDefaultSystem
      (system:
        let
          overlays = [
            (import rust-overlay)
            (import ./rocksdb-overlay.nix)
          ];
          pkgs = import nixpkgs {
            inherit system overlays;
          };
          rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

          crateName = craneLib.crateNameFromCargoToml {
            cargoToml = ./cli/Cargo.toml;
          };

          # src = craneLib.cleanCargoSource ./.; # filter out md files which are used in docs
          src = nixpkgs.lib.cleanSource (craneLib.path ./.);

          nativeBuildInputs = with pkgs; [ rustToolchain clang ];
          buildInputs = with pkgs; [ ];
          commonArgs = {
            inherit (crateName) pname version;
            inherit src buildInputs nativeBuildInputs;
            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib"; # for rocksdb

            # link rocksdb dynamically
            ROCKSDB_INCLUDE_DIR = "${pkgs.rocksdb}/include";
            ROCKSDB_LIB_DIR = "${pkgs.rocksdb}/lib";

            cargoExtraArgs = "--all-features";
          };

          # Building only dependencies, this will not be rebuilt if local src changes
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          # Run clippy (and deny all warnings) on the crate source,
          # reusing the dependency artifacts (e.g. from build scripts or
          # proc-macros) from above.
          #
          # Note that this is done as a separate derivation so it
          # does not impact building just the crate by itself.
          clippy = craneLib.cargoClippy (commonArgs // {
            # Again we apply some extra arguments only to this derivation
            # and not every where else. In this case we add some clippy flags
            inherit cargoArtifacts;
            cargoClippyExtraArgs = "--all-targets -- --deny warnings"; # --all-features already defined from cargoExtraArgs
          });


          # Also run the crate tests under cargo-tarpaulin so that we can keep
          # track of code coverage
          tarpaulin = craneLib.cargoTarpaulin (commonArgs // {
            inherit cargoArtifacts;

          });

          bin = craneLib.buildPackage (commonArgs // {
            inherit cargoArtifacts;
          });
        in
        with pkgs;
        {
          packages = {
            inherit bin;
            default = bin;
            blocks_iterator = bin;
          };
          checks = {
            inherit bin clippy tarpaulin;
          };
          devShells.default = mkShell {
            inputsFrom = [ bin ];

            LIBCLANG_PATH = "${pkgs.libclang.lib}/lib"; # for rocksdb

            # link rocksdb dynamically
            ROCKSDB_INCLUDE_DIR = "${pkgs.rocksdb}/include";
            ROCKSDB_LIB_DIR = "${pkgs.rocksdb}/lib";

            buildInputs = with pkgs; [ ];
          };
        }
      );
}
