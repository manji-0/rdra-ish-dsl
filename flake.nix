{
  description = "rdra-ish: RDRA-inspired DSL compiler and diagram generator";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      systems = [
        "aarch64-darwin"
        "x86_64-darwin"
        "aarch64-linux"
        "x86_64-linux"
      ];
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );
    in
    {
      packages = forAllSystems (pkgs: {
        rdra-ish = pkgs.rustPlatform.buildRustPackage {
          pname = "rdra-ish";
          version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;
          src = self;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [
            "-p"
            "rdra-ish-cli"
          ];
          cargoTestFlags = [ "--workspace" ];
          meta = {
            description = "RDRA-inspired DSL compiler and diagram generator";
            license = pkgs.lib.licenses.mit;
            mainProgram = "rdra-ish";
          };
        };

        default = self.packages.${pkgs.stdenv.hostPlatform.system}.rdra-ish;
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = [
            (pkgs.rust-bin.stable.latest.default.override {
              extensions = [
                "rust-src"
                "rust-analyzer"
              ];
            })
            pkgs.cargo-insta

            # Python packaging (maturin bin bindings, uv publish flow)
            pkgs.maturin
            pkgs.uv
            pkgs.python3

            # Rendering and formal verification
            pkgs.jdk17_headless
            pkgs.plantuml
            pkgs.tlaplus

            # VS Code extension (editors/vscode)
            pkgs.nodejs

            pkgs.pre-commit
          ];

          JAVA_HOME = "${pkgs.jdk17_headless}";
          PLANTUML_JAR = "${pkgs.plantuml}/lib/plantuml.jar";
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
