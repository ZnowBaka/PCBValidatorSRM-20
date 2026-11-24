

{
  description = "Reusable Rust development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
  };

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
      };
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [
          # Rust
          rustc
          cargo
          rustfmt
          clippy
          rust-analyzer

          # Build tools
          pkg-config
          gcc
          gnumake

          # Wayland / egui / winit
          wayland
          wayland-protocols
          libxkbcommon

          # X11 compatibility
          xorg.libX11
          xorg.libXcursor
          xorg.libXrandr
          xorg.libXi

          # Common GUI dependencies
          vulkan-loader
        ];

        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [
          pkgs.wayland
          pkgs.libxkbcommon
          pkgs.vulkan-loader
          pkgs.libGL
        ];

        shellHook = ''
          echo "Rust development environment loaded"
          rustc --version
        '';
      };
    };
}
