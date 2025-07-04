{
  description = "Small Turtle House Account API";

  inputs = {
    nixpkgs.url = "nixpkgs";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  # Based on https://github.com/oxalica/rust-overlay
  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
      crane,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        # Input pkgs
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # Setup crane with toolchain
        rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

        # crane define src
        src = craneLib.cleanCargoSource ./.;

        nativeBuildInputs = [
          pkgs.pkg-config
        ];

        buildInputs = [
          pkgs.openssl # for rustls
        ];

        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;

        # build artifacts
        commonArgs = {
          inherit src nativeBuildInputs buildInputs;
        };

        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        unstrippedBin = craneLib.buildPackage (
          commonArgs
          // {
            inherit cargoArtifacts;
          }
        );

        bin = pkgs.stdenv.mkDerivation {
          name = "${unstrippedBin.name}-stripped";
          src = unstrippedBin;
          nativeBuildInputs = [ pkgs.binutils ]; # for 'strip'
          installPhase = ''
            mkdir -p $out/bin
            strip -o $out/bin/sth-account $src/bin/sth-account
          '';
        };

        dockerImage = pkgs.dockerTools.streamLayeredImage {
          name = "sth-account";
          tag = "latest";
          contents = [
            bin
            pkgs.cacert
          ];
          config = {
            Cmd = [ "${bin}/bin/sth-account" ];
          };
        };
      in
      with pkgs;
      {
        devShells.default = mkShell {
          inherit LD_LIBRARY_PATH;
          buildInputs = [
            rustToolchain
            openssl
            pkgs.sea-orm-cli
            dive
            just
          ];
          nativeBuildInputs = [
            pkg-config
          ];
        };
        packages = {
          inherit bin dockerImage;
          default = bin;
        };
      }
    );
}
