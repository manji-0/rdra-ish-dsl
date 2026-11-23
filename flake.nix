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
            "-p"
            "rdra-ish-lsp"
          ];
          # The workspace tests call python3, tlc, z3, tsc and go; CI runs them in the devShell.
          doCheck = false;
          # `verify` runs tlc, and `verify --prove` runs z3, from PATH. Appended, so a tlc or z3
          # the user already has comes first.
          nativeBuildInputs = [ pkgs.makeWrapper ];
          postInstall = ''
            wrapProgram $out/bin/rdra-ish \
              --suffix PATH : ${
                pkgs.lib.makeBinPath [
                  pkgs.tlaplus
                  pkgs.z3
                ]
              }
          '';
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

            # Formal verification (TLC)
            pkgs.jdk17_headless
            pkgs.tlaplus
            # `verify --prove` (tests/prove_cli.rs, tests/proof_equivalence.rs)
            pkgs.z3

            # VS Code extension (editors/vscode)
            pkgs.nodejs

            # Compile checks of `rdra-ish codegen` output (tests/codegen_builds.rs)
            pkgs.typescript
            pkgs.go

            pkgs.pre-commit
            # Mutation run over the liveness ladder (.cargo/mutants.toml, .github/workflows/mutants.yml)
            pkgs.cargo-mutants
          ]
          # GNU time for scripts/verify-samples.sh (macOS has BSD /usr/bin/time).
          ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.time ];

          JAVA_HOME = "${pkgs.jdk17_headless}";

          # A GOROOT from the user's shell (a version manager's Go, say) points the flake's
          # `go` at another toolchain's standard library, and the codegen build tests then
          # fail on a version mismatch. Without it, `go` finds its own root.
          shellHook = ''
            unset GOROOT
          '';
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
