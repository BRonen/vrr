{
  description = "Viewstamped Replication Revisited";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachSystem [ "x86_64-darwin" "x86_64-linux" "i686-linux" ] (system:
      let pkgs = import nixpkgs { inherit system; };
      in {
        defaultPackage = pkgs.mkShell {
          buildInputs = with pkgs;[ rustc cargo rust-analyzer bacon pkg-config openssl.dev tlaplus ];

          shellHook = "cargo --version & rustc --version";
        };
      }
    );
}
