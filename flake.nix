{
  description = "番号付き worktree 開発コンテナへの docker exec ラッパー";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixpkgs-26.05-darwin";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "dwt";
          version = "0.3.0";
          src = self;
          cargoLock.lockFile = ./Cargo.lock;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            cargo
            rustc
            rustfmt
            clippy
            rust-analyzer
          ];
        };
      });
}
