{
  description = "Pylon - LLM API Gateway for OpenZerg";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, flake-utils, crane, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        craneLib = crane.mkLib pkgs;

        src = craneLib.cleanCargoSource ./.;

        commonArgs = {
          inherit src;
          buildInputs = with pkgs; [ openssl ];
        };

        cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
          pname = "pylon-deps";
        });

        pylon = craneLib.buildPackage (commonArgs // {
          inherit cargoArtifacts;
          pname = "pylon";
          doCheck = false;
        });

      in
      {
        packages = {
          inherit pylon;
          default = pylon;
        };

        devShells.default = craneLib.devShell {
          inherit src;
          inputsFrom = [ pylon ];
          packages = with pkgs; [ rust-analyzer cargo-watch ];
        };
      }
    );
}