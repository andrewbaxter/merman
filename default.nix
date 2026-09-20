# Builds the merman3 cli (with the wasm viewer embedded) using crane.
#
#   nix-build          -> result/bin/merman3
#   nix-build -A web   -> just the wasm-bindgen output (merman3_web.js, merman3_web_bg.wasm)
#
# The crates are staged into a tree with only the files the build reads, so
# unrelated changes in the repository don't invalidate the build.
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

  # The workspace with only what the build reads.
  stagedSources = pkgs.runCommand "merman3-sources" { } ''
    mkdir -p $out/cli/static
    cp ${./Cargo.toml} $out/Cargo.toml
    cp ${./Cargo.lock} $out/Cargo.lock
    cp -r ${./core} $out/core
    cp -r ${./web} $out/web
    cp -r ${./cli/src} $out/cli/src
    cp ${./cli/Cargo.toml} $out/cli/Cargo.toml
    cp ${./cli/static/index.html} $out/cli/static/index.html
    cp -r ${./syntaxes} $out/syntaxes
  '';

  webArgs = {
    pname = "merman3_web";
    inherit version;
    src = stagedSources;
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
    cp ${web}/merman3_web.js ${web}/merman3_web_bg.wasm $out/cli/static/
  '';

  # The webview window (wry/tao) needs gtk and webkitgtk.
  cliNativeBuildInputs = [ pkgs.pkg-config ];
  cliBuildInputs = [
    pkgs.glib
    pkgs.gtk3
    pkgs.webkitgtk_4_1
    pkgs.libsoup_3
    pkgs.dbus
    pkgs.cairo
    pkgs.pango
    pkgs.gdk-pixbuf
    pkgs.atk
    pkgs.xdotool
  ];

  cliArgs = {
    pname = "merman3";
    inherit version;
    src = stagedSources;
    cargoLock = ./Cargo.lock;
    cargoExtraArgs = "--locked -p merman3";
    cargoTestExtraArgs = "-p merman3_core";
    nativeBuildInputs = cliNativeBuildInputs;
    buildInputs = cliBuildInputs;
  };
  cli = craneLib.buildPackage (
    cliArgs
    // {
      src = stagedSourcesWithWeb;
      cargoArtifacts = craneLib.buildDepsOnly cliArgs;
      # The binary launches the webview at runtime, so it needs the gtk
      # environment (gsettings schemas, gdk-pixbuf loaders) wrapped in.
      nativeBuildInputs = cliNativeBuildInputs ++ [ pkgs.wrapGAppsHook3 ];
      passthru = {
        inherit web;
      };
    }
  );
in
cli
