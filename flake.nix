{
  description = "Screenshot capture and upload tool for Linux and macOS";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { nixpkgs, ... }:
    let
      forAllSystems = nixpkgs.lib.genAttrs [ "x86_64-linux" "aarch64-linux" ];
      nativeBuildInputs = pkgs: with pkgs; [ cmake perl pkg-config ];
      buildInputs = pkgs: [ pkgs.libxkbcommon ];
      editorInputs = pkgs: with pkgs; [
        webkitgtk_4_1 gtk3 libsoup_3 openssl glib cairo pango gdk-pixbuf atk librsvg
        glib-networking gsettings-desktop-schemas
      ];
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          webUi = pkgs.buildNpmPackage {
            pname = "accioshot-web";
            version = "0.1.0";
            src = pkgs.lib.cleanSourceWith {
              src = ./ui/web;
              filter = path: _: !(builtins.elem (baseNameOf path) [ "node_modules" "dist" ]);
            };
            npmDepsHash = "sha256-teUftjgEXqmD+tGXPRDPSlf4XqhqEWXD2RJ42rXVK+Q=";
            installPhase = "cp -r dist $out";
          };
        in {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "accioshot";
            version = "0.1.0";
            src = pkgs.lib.cleanSourceWith {
              src = ./.;
              filter = path: _: !(builtins.elem (baseNameOf path) [ "target" "node_modules" "dist" "result" ]);
            };
            cargoLock.lockFile = ./Cargo.lock;
            cargoBuildFlags = [ "--workspace" ];
            cargoTestFlags = [ "--workspace" ];
            nativeBuildInputs = nativeBuildInputs pkgs ++ [ pkgs.wrapGAppsHook3 ];
            buildInputs = buildInputs pkgs ++ editorInputs pkgs;
            preBuild = "cp -r ${webUi} ui/web/dist";
            postInstall = ''
              install -Dm644 contrib/accioshot.service $out/share/systemd/user/accioshot.service
              substituteInPlace $out/share/systemd/user/accioshot.service \
                --replace-fail /usr/bin/accioshot $out/bin/accioshot
            '';
            meta.mainProgram = "accioshot";
          };
        });

      devShells = forAllSystems (system:
        let pkgs = nixpkgs.legacyPackages.${system};
        in {
          default = pkgs.mkShell {
            nativeBuildInputs = nativeBuildInputs pkgs
              ++ (with pkgs; [ cargo rustc rustfmt clippy rust-analyzer nodejs ]);
            buildInputs = buildInputs pkgs ++ editorInputs pkgs;
            shellHook = ''
              export GIO_EXTRA_MODULES=${pkgs.glib-networking}/lib/gio/modules
              export XDG_DATA_DIRS=${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}:$XDG_DATA_DIRS
            '';
          };
        });
    };
}
