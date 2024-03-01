{
  description = "zack kb gui thing";

  inputs = {
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.follows = "rust-overlay/flake-utils";
    nixpkgs.follows = "rust-overlay/nixpkgs";
  };

  outputs = inputs:
    with inputs;
      flake-utils.lib.eachDefaultSystem (
        system: let
          pkgs = nixpkgs.legacyPackages.${system};
          code = pkgs.callPackage ./. {inherit nixpkgs system naersk rust-overlay;};
        in rec {
          packages = {
            kb = pkgs.rustPlatform.buildRustPackage {
              pname = "kb";
              version = "0.0.1";
              src = ./.;

              cargoLock = {
                lockFile = ./Cargo.lock;
              };

              nativeBuildInputs = [pkgs.pkg-config];
              buildInputs = [pkgs.systemd];
            };
          };
        }
      );
}
