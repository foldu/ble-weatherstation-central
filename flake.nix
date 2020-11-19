{
  description = "A thing.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    naersk = {
      url = "github:nmattia/naersk";
      inputs.nixpkgs.follows = "/nixpkgs";
    };
  };

  outputs = { self, nixpkgs, naersk, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        nativeBuildInputs = with pkgs; [
          cargo
          rustc
          yarn
        ];
      in
      {
        defaultPackage = naersk.lib.${system}.buildPackage {
          src = ./.;
          inherit nativeBuildInputs;
        };
        defaultApp = {
          type = "app";
          program = "${self.defaultPackage.${system}}/bin/ble-weathersensor-central";
        };
        devShell = pkgs.mkShell {
          inherit nativeBuildInputs;
        };
      }
    );
}
