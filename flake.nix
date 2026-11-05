{
  description = "Screenshot capture and upload tool for Linux";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { nixpkgs, ... }:
    let
      forAllSystems = nixpkgs.lib.genAttrs [ "x86_64-linux" "aarch64-linux" ];
      nativeBuildInputs = pkgs: with pkgs; [ cmake perl pkg-config ];
      buildInputs = pkgs: [ pkgs.libxkbcommon ];
    in
    {
      packages = forAllSystems (system:
        let pkgs = nixpkgs.legacyPackages.${system};
        in {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "accioshot";
            version = "0.1.0";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = nativeBuildInputs pkgs;
            buildInputs = buildInputs pkgs;
            meta.mainProgram = "accioshot";
          };
        });

      devShells = forAllSystems (system:
        let pkgs = nixpkgs.legacyPackages.${system};
        in {
          default = pkgs.mkShell {
            nativeBuildInputs = nativeBuildInputs pkgs
              ++ (with pkgs; [ cargo rustc rustfmt clippy rust-analyzer ]);
            buildInputs = buildInputs pkgs;
          };
        });
    };
}
