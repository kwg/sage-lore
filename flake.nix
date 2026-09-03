{
  description = "sage-lore — LLM Orchestration Engine for the SAGE Method";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; config.allowUnfree = true; };
      in
      {
        packages.default = self.packages.${system}.sage-lore;

        packages.sage-lore = pkgs.rustPlatform.buildRustPackage {
          pname = "sage-lore";
          version = "1.0.0-beta";
          src = ./.;

          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = with pkgs; [ pkg-config git ];
          buildInputs = with pkgs; [ openssl ];

          checkFlags = [
            "--skip=primitives::test::verify::tests"
            "--skip=primitives::vcs::merge::tests"
            "--skip=primitives::vcs::stash::tests"
          ];

          preCheck = ''
            export HOME=$(mktemp -d)
            git config --global user.email "test@example.com"
            git config --global user.name "Test"
          '';

          SAGE_LORE_DATADIR = "${placeholder "out"}/share/sage-lore";

          postInstall = ''
            mkdir -p $out/share/sage-lore/scrolls
            cp -r scrolls/* $out/share/sage-lore/scrolls/

            mkdir -p $out/share/sage-lore/agents
            cp -r agents/* $out/share/sage-lore/agents/

            mkdir -p $out/share/sage-lore/config/security
            cp ${./share/config/config.yaml} $out/share/sage-lore/config/config.yaml
            cp ${./share/config/security/policy.yaml} $out/share/sage-lore/config/security/policy.yaml
          '';

          meta = with pkgs.lib; {
            description = "LLM Orchestration Engine — deterministic scroll execution for AI workflows";
            homepage = "https://github.com/kai/sage-lore";
            license = licenses.mit;
            mainProgram = "sage-lore";
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            nodejs_22
            jq
            yq-go
            shellcheck
            git
            rustc
            cargo
            clippy
            rust-analyzer
            pkg-config
            openssl
            python312
          ];
          shellHook = ''
            echo "SAGE Method dev environment loaded"
          '';
        };
      });
}
