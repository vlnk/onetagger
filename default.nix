{ stdenv, pkgs, nixpkgs, system, makeRustPlatform, rust-overlay, nodejs, pnpm_9 }:


let client = stdenv.mkDerivation rec {
      pname = "onetagger-client";
      version = "1.7.0";

      nativeBuildInputs = [
        nodejs
        pnpm_9.configHook
      ];

      src = ./client;


      pnpmDeps = pnpm_9.fetchDeps {
        inherit pname src;
        hash = "sha256-yqEBB2V+i2Tq3l/PfPffyNLe4SiXGXJSggTJi7R5tic=";
      };

      postBuild = ''
        pnpm run build
      '';

      installPhase = ''
        runHook preInstall

        mkdir -p $out/share
        mv dist $out/share/${pname}

        runHook postInstall
  '';
    };
in pkgs.rustPlatform.buildRustPackage rec {
  pname = "onetagger";
  version = "1.7.0";
  src = ./.;

  cargoHash = "";

  nativeBuildInputs = with pkgs; [
    rustc
    cargo
    lld
    autogen
    pkg-config
    openssl
    libgcc
    glib
    autoPatchelfHook
  ];

  buildInputs = with pkgs; [
    nodejs
    pnpm
    curl
    gnumake
    pango
    cairo
    gdk-pixbuf
    gtk3
    libsoup_3
    webkitgtk_4_1
    alsa-lib
    portaudio
    client
  ];

  # https://nixos.org/manual/nixpkgs/stable/#fun-substitute
  # https://discourse.nixos.org/t/how-to-escape-in-indented-string/41114
  prePatch = ''
    substituteInPlace crates/onetagger-ui/src/lib.rs \
        --replace-fail \$CARGO_MANIFEST_DIR/../../client/dist ${client}/share/onetagger-client
  '';

  cargoLock = {
    lockFile = ./Cargo.lock;

    outputHashes = {
      "songrec-0.2.1" = "sha256-pQKU99x52cYQjBVctsI4gdju9neB6R1bluL76O1MZMw=";
    };
  };

  checkFlags = [
    # skip all tests that requires network
    "--skip=bandcamp::test_bandcamp"
    "--skip=beatsource::test_beatsource"
    "--skip=musicbrainz::tests::test_search_serialization"
    "--skip=musicbrainz::tests::text_full_release_serialization"
  ];

  #postFixup = with pkgs; ''
  #  patchelf \
  #    --add-needed ${alsa-lib}/lib/libasound.so.2 \
  #    --add-needed ${webkitgtk_4_1}/lib/libwebkit2gtk-4.1.so.0 \
  #    --add-needed ${gtk3}/lib/libgtk-3.so.0 \
  #    --add-needed ${gtk3}/lib/libgdk-3.so.0 \
  #    --add-needed ${cairo}/lib/libcairo.so.2 \
  #    --add-needed ${gdk-pixbuf}/lib/libgdk_pixbuf-2.0.so.0 \
  #    --add-needed ${libsoup_3}/lib/libsoup-3.0.so.0 \
  #    --add-needed ${glib}/lib/libgio-2.0.so.0 \
  #    "$out/bin/onetagger"
  #'';
}
