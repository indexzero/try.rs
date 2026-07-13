{
  description = "try - fresh directories for every vibe (Rust port; binary: tryme)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    crane.url = "github:ipetkov/crane/v0.23.4";
  };

  outputs = inputs@{ flake-parts, crane, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];

      flake = {
        # Home Manager module — the option contract is upstream tobi/try's,
        # verbatim: programs.try.{enable,package,path}. Only the internals
        # changed (the binary is `tryme`; the emitted shell function is
        # still `try`, so user config and muscle memory carry over 1:1).
        homeModules.default = { config, lib, pkgs, ... }:
          with lib;
          let
            cfg = config.programs.try;
          in
          {
            options.programs.try = {
              enable = mkEnableOption "try - fresh directories for every vibe";

              package = mkOption {
                type = types.package;
                default = inputs.self.packages.${pkgs.stdenv.hostPlatform.system}.default;
                defaultText = literalExpression "inputs.self.packages.\${pkgs.stdenv.hostPlatform.system}.default";
                # Upstream's description documents a Ruby `.override`; that
                # affordance doesn't exist for a crane-built Rust package, so
                # the text deviates deliberately rather than document a lie.
                description = "The try-me-maybe package to use.";
              };

              path = mkOption {
                type = types.str;
                default = "~/src/tries";
                description = "Path where try directories will be stored.";
              };
            };

            config = mkIf cfg.enable {
              programs.bash.initExtra = mkIf config.programs.bash.enable ''
                eval "$(${cfg.package}/bin/tryme init ${cfg.path})"
              '';

              programs.zsh.initContent = mkIf config.programs.zsh.enable ''
                eval "$(${cfg.package}/bin/tryme init ${cfg.path})"
              '';

              programs.fish.shellInit = mkIf config.programs.fish.enable ''
                eval (${cfg.package}/bin/tryme init ${cfg.path} | string collect)
              '';
            };
          };

        # Backwards compatibility - deprecated (upstream parity)
        homeManagerModules.default = builtins.trace
          "WARNING: homeManagerModules is deprecated and will be removed in a future version. Please use homeModules instead."
          inputs.self.homeModules.default;
      };

      perSystem = { config, self', inputs', pkgs, system, ... }:
        let
          craneLib = crane.mkLib pkgs;

          commonArgs = {
            # Root Cargo.toml is a virtual workspace and the crates inherit
            # version.workspace = true, which crateNameFromCargoToml cannot
            # resolve — read the workspace table directly.
            pname = "try-me-maybe";
            version =
              (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version;
            src = craneLib.cleanCargoSource ./.;
            strictDeps = true;
          };

          cargoArtifacts = craneLib.buildDepsOnly commonArgs;

          tryme = craneLib.buildPackage (commonArgs // {
            inherit cargoArtifacts;
            # cargo tests are the cargo CI jobs' gate; the flake's own check
            # is the conformance suite below. They also cannot pass here by
            # construction: cleanCargoSource strips non-cargo files, so the
            # usage-spec freshness and oracle fixtures are absent.
            doCheck = false;
            meta = with pkgs.lib; {
              description = "Fresh directories for every vibe - a byte-conformant Rust port of tobi/try";
              homepage = "https://github.com/indexzero/try.rs";
              license = licenses.mit;
              mainProgram = "tryme";
              platforms = platforms.unix;
            };
          });
        in
        {
          packages.default = tryme;

          apps.default = {
            type = "app";
            program = "${tryme}/bin/tryme";
          };

          # `nix flake check` runs the adopted upstream conformance suite
          # against the built binary. bash + zsh legs run in the sandbox;
          # the fish leg self-skips (it hardcodes nix-shell, unavailable
          # inside the build sandbox — see ADR notes in the repo).
          checks = {
            inherit tryme;

            conformance = pkgs.stdenv.mkDerivation {
              pname = "tryme-conformance";
              version = tryme.version or "0";
              src = ./.;
              nativeBuildInputs = [ pkgs.bash pkgs.zsh pkgs.coreutils ];
              dontBuild = true;
              doCheck = true;
              checkPhase = ''
                patchShebangs spec/tests scripts
                # Same gate as CI: the shrink-only ratchet (a raw runner call
                # would go red on spec-sync PRs that legitimately raise the
                # baseline before the port catches up)
                bash scripts/conformance-ratchet.sh ${tryme}/bin/tryme
              '';
              installPhase = "touch $out";
            };
          };
        };
    };
}
