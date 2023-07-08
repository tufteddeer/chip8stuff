{
  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, flake-utils, naersk, nixpkgs, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = (import nixpkgs) {
          inherit system overlays;
        };

        toolchain = pkgs.rust-bin.selectLatestNightlyWith
          (toolchain: toolchain.default.override {
            extensions = [ "rust-src" ];
          });

        naersk' = pkgs.callPackage naersk {
          cargo = toolchain;
          rustc = toolchain;
        };

        buildInputs = with pkgs; [
          xorg.libX11
          xorg.libXcursor
          xorg.libXrandr
          xorg.libXi

          vulkan-loader

          makeWrapper
        ];
      in
      rec {
        # For `nix build` & `nix run`:
        defaultPackage = naersk'.buildPackage
          {
            src = ./.;
            nativeBuildInputs = with pkgs; [
              xorg.libX11
            ];

            buildInputs = buildInputs;

            # to prevent missing .so files when executing the file normally (without 'nix run') (see https://github.com/Anton-4/winit_nix/blob/main/flake.nix)
            postInstall = ''
              wrapProgram $out/bin/chip8stuff --set LD_LIBRARY_PATH "${pkgs.lib.makeLibraryPath buildInputs}"
            '';

            # LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath buildInputs}";
          };

        # For `nix develop` (optional, can be skipped):
        devShell = pkgs.mkShell
          {
            nativeBuildInputs = with pkgs; [
              toolchain
            ];

            buildInputs = buildInputs;

            LD_LIBRARY_PATH = "${nixpkgs.lib.makeLibraryPath buildInputs}";
          };
      }
    );
}
