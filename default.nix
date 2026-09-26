# Builds the merman cli (with the wasm viewer embedded) using crane.
#
#   nix-build          -> result/bin/merman
#   nix-build -A web   -> just the wasm-bindgen output (merman_web.js, merman_web_bg.wasm)
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

  # The claude cli the ai pane runs, at whatever version npm currently calls latest.
  claudeCodePlatform =
    {
      x86_64-linux = "linux-x64";
      aarch64-linux = "linux-arm64";
      x86_64-darwin = "darwin-x64";
      aarch64-darwin = "darwin-arm64";
    }
    .${pkgs.stdenv.hostPlatform.system};
  claudeCodeLatest = builtins.fromJSON (
    builtins.readFile (
      builtins.fetchurl "https://registry.npmjs.org/@anthropic-ai/claude-code-${claudeCodePlatform}/latest"
    )
  );
  claudeCode = pkgs.stdenv.mkDerivation {
    pname = "claude-code";
    version = claudeCodeLatest.version;
    src = pkgs.fetchurl {
      url = claudeCodeLatest.dist.tarball;
      hash = claudeCodeLatest.dist.integrity;
    };
    nativeBuildInputs = [ pkgs.autoPatchelfHook ];
    buildInputs = [ pkgs.stdenv.cc.cc.lib ];
    dontStrip = true;
    installPhase = ''
      mkdir -p $out/bin
      install -m755 claude $out/bin/claude
    '';
  };

  # The workspace with only what the build reads.
  stagedSources = pkgs.runCommand "merman-sources" { } ''
    mkdir -p $out/cli/static
    cp ${./Cargo.toml} $out/Cargo.toml
    cp ${./Cargo.lock} $out/Cargo.lock
    cp -r ${./core} $out/core
    cp -r ${./api} $out/api
    cp -r ${./web} $out/web
    cp -r ${./cli/src} $out/cli/src
    cp ${./cli/Cargo.toml} $out/cli/Cargo.toml
    cp ${./cli/static/demo.html} $out/cli/static/demo.html
    cp ${./cli/static/editor.html} $out/cli/static/editor.html
    cp ${./cli/static/merman.css} $out/cli/static/merman.css
    cp ${./cli/static/MaterialIcons-Regular.ttf} $out/cli/static/MaterialIcons-Regular.ttf
    cp -r ${./syntaxes} $out/syntaxes
  '';

  webArgs = {
    pname = "merman_web";
    inherit version;
    src = stagedSources;
    cargoLock = ./Cargo.lock;
    cargoExtraArgs = "--locked -p merman_web";
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
        wasm-bindgen --target web --no-typescript --out-dir $out --out-name merman_web \
          target/wasm32-unknown-unknown/release/merman_web.wasm
      '';
    }
  );

  # The staged tree plus the built viewer, which the cli embeds.
  stagedSourcesWithWeb = pkgs.runCommand "merman-sources-with-web" { } ''
    cp -r ${stagedSources} $out
    chmod -R u+w $out
    cp ${web}/merman_web.js ${web}/merman_web_bg.wasm $out/cli/static/
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
    pname = "merman";
    inherit version;
    src = stagedSources;
    cargoLock = ./Cargo.lock;
    cargoExtraArgs = "--locked -p merman";
    cargoTestExtraArgs = "-p merman_core";
    nativeBuildInputs = cliNativeBuildInputs;
    buildInputs = cliBuildInputs;
  };
  cli = craneLib.buildPackage (
    cliArgs
    // {
      src = stagedSourcesWithWeb;
      cargoArtifacts = craneLib.buildDepsOnly cliArgs;
      # The binary launches the webview at runtime, so it needs the gtk
      # environment (gsettings schemas, gdk-pixbuf loaders) wrapped in. The ai
      # pane's sandbox needs claude and merman-tool on the PATH and, since every
      # binary here lives in the store, the store bound read-only inside it.
      nativeBuildInputs = cliNativeBuildInputs ++ [ pkgs.wrapGAppsHook3 ];
      preFixup = ''
        gappsWrapperArgs+=(--prefix PATH : "${claudeCode}/bin:$out/bin" --set MERMAN_SANDBOX_RO /nix/store)
      '';
      passthru = {
        inherit web claudeCode;
      };
    }
  );
in
cli
