# Builds the merman3 cli (with the wasm viewer embedded) using crane.
#
#   nix-build          -> result/bin/merman3
#   nix-build -A web   -> just the wasm-bindgen output (merman3_web.js, merman3_web_bg.wasm)
#
# The crates are staged into a copy of the repository layout so the core tests
# can read the ecosystem `.at` modules they reference relative to the crate.
{
  pkgs ? import <nixpkgs> { },
}:
let
  craneSrc = builtins.fetchTarball {
    url = "https://github.com/ipetkov/crane/archive/v0.21.1.tar.gz";
    sha256 = "1ci6p7z9izshqbfg58bba6yvvhldgs4dk2gny6i5xh2z8xl5ckim";
  };
  craneLib = import craneSrc { inherit pkgs; };
  version = "0.1.0";
  repoRoot = ../..;

  # Repository layout with only what the build reads: the workspace under
  # editor/merman3 and the ecosystem modules the tests load.
  stagedSources = pkgs.runCommand "merman3-sources" { } ''
    m=$out/editor/merman3
    mkdir -p $m/cli/static $out/ecosystem
    cp ${./Cargo.toml} $m/Cargo.toml
    cp ${./Cargo.lock} $m/Cargo.lock
    cp -r ${./core} $m/core
    cp -r ${./web} $m/web
    cp -r ${./cli/src} $m/cli/src
    cp ${./cli/Cargo.toml} $m/cli/Cargo.toml
    cp ${./cli/static/index.html} $m/cli/static/index.html
    cp -r ${./syntaxes} $m/syntaxes
    cp ${repoRoot + "/ecosystem"}/*.at $out/ecosystem/
  '';

  # Enter the workspace inside the staged tree.
  enterWorkspace = ''
    cd "$sourceRoot/editor/merman3"
    sourceRoot="."
  '';

  webArgs = {
    pname = "merman3_web";
    inherit version;
    src = stagedSources;
    postUnpack = enterWorkspace;
    cargoLock = ./Cargo.lock;
    cargoExtraArgs = "--locked -p merman3_web";
    CARGO_BUILD_TARGET = "wasm32-unknown-unknown";
    doCheck = false;
    nativeBuildInputs = [ pkgs.lld ];
  };
  web = craneLib.buildPackage (
    webArgs
    // {
      cargoArtifacts = craneLib.buildDepsOnly webArgs;
      nativeBuildInputs = [
        pkgs.lld
        pkgs.wasm-bindgen-cli_0_2_121
      ];
      installPhaseCommand = ''
        mkdir -p $out
        wasm-bindgen --target web --no-typescript --out-dir $out --out-name merman3_web \
          target/wasm32-unknown-unknown/release/merman3_web.wasm
      '';
    }
  );

  # The staged tree plus the built viewer, which the cli embeds.
  stagedSourcesWithWeb = pkgs.runCommand "merman3-sources-with-web" { } ''
    cp -r ${stagedSources} $out
    chmod -R u+w $out
    cp ${web}/merman3_web.js ${web}/merman3_web_bg.wasm $out/editor/merman3/cli/static/
  '';

  cliArgs = {
    pname = "merman3";
    inherit version;
    src = stagedSources;
    postUnpack = enterWorkspace;
    cargoLock = ./Cargo.lock;
    cargoExtraArgs = "--locked -p merman3";
    cargoTestExtraArgs = "-p merman3_core";
  };
  cli = craneLib.buildPackage (
    cliArgs
    // {
      src = stagedSourcesWithWeb;
      cargoArtifacts = craneLib.buildDepsOnly cliArgs;
      passthru = {
        inherit web;
      };
    }
  );
in
cli
