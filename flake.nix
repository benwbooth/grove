{
  description = "grove: a Finder-style file manager with Dolphin's power (Rust + Qt Quick)";

  # The same nixpkgs revision as /etc/nixos, so grove uses the Qt build the
  # desktop's style and platform plugins were made for.
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/ab5c18fdedfc8f9bbc9d773f10a540790e2787a7";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      # (qtwebengine: it carries QtPdf, for PDF previews.)
      qtFor = pkgs: with pkgs.qt6; [ qtbase qtdeclarative qtsvg qtwayland qtmultimedia qtimageformats qtwebengine ];
      qtEnvFor = pkgs: pkgs.qt6.env "qt-grove-${pkgs.qt6.qtbase.version}" ((builtins.tail (qtFor pkgs)) ++ [ pkgs.libGL ]);
      # Small standard tools grove runs (thumbnails, archives, remote
      # places, devices, git status). All optional at run time.
      # Libraries grove links instead of running their command-line tools
      # (docs/NO_EXTERNAL_TOOLS.md).
      linkedFor = pkgs: with pkgs; [ libarchive libgit2 libssh2 openssl zlib ffmpeg-headless vips glib ];
      runtimeFor = pkgs: with pkgs; [
        ffmpegthumbnailer libheif git sshfs rclone simple-mtpfs
        poppler-utils vips imagemagick djvulibre mupdf ghostscript magika-cli pandoc
        udisks fuse3 avahi cifs-utils smbnetfs ffmpeg-headless xorriso dvdplusrwtools
      ];
    in
    {
      packages = forAll (pkgs:
        let
          qtEnv = qtEnvFor pkgs;
          cargoDeps = pkgs.rustPlatform.importCargoLock { lockFile = ./Cargo.lock; };
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = path: type: !(pkgs.lib.hasInfix "/target" path) && baseNameOf path != ".qt-env" && baseNameOf path != "result";
          };
          # Build in a persistent target directory (/var/tmp/cargo-nix, exposed
          # to the sandbox by the system flake) so rebuilds only recompile what
          # changed. Falls back to a throwaway directory when it isn't there.
          # kache stays out of these builds: CARGO_HOME is the build's own.
          persistentCargo = name: cargoArgs: ''
            runHook preBuild
            workDir=$TMPDIR/cargo-nix
            if [ -w /var/tmp/cargo-nix ]; then
              workDir=/var/tmp/cargo-nix/${name}
            fi
            # The store vendor copy only changes with Cargo.lock; the per-build
            # copy gets new mtimes, which would rebuild every dependency.
            sed -i "s|^directory = .*|directory = \"${cargoDeps}\"|" "$NIX_BUILD_TOP/.cargo/config.toml"
            (
              # Writable by everyone: each build may run as a different nixbld
              # user. Kept to this subshell so the outputs get normal modes.
              umask 000
              mkdir -p "$workDir/src" "$workDir/target"
              touch "$workDir"
              # Only rewrite files whose content changed (cargo checks mtimes).
              rsync -rc --no-times --no-perms --chmod=a+rwX --delete ./ "$workDir/src/"
              # The linker wrapper rejects output paths outside the build dir.
              export NIX_ENFORCE_PURITY=0
              cd "$workDir/src"
              # cxx-qt copies its QML exports with fs::copy, which also
              # chmods the destination: that fails on a file left by another
              # nixbld user. The exports are regenerated every build anyway.
              rm -rf "$workDir/target/cxxqt"
              CARGO_HOME="$NIX_BUILD_TOP/.cargo" CARGO_TARGET_DIR="$workDir/target" \
                cargo build ${cargoArgs} --offline --locked -j "$NIX_BUILD_CORES"
              find "$workDir/target" -mindepth 1 -user "$(id -u)" ! -perm -o+w -exec chmod a+rwX {} + 2>/dev/null || true
            )
            ln -sfn "$workDir/target" target
            runHook postBuild
          '';
          grove = pkgs.rustPlatform.buildRustPackage {
            pname = "grove";
            version = (pkgs.lib.importTOML ./Cargo.toml).package.version;
            inherit src cargoDeps;
            # bindgenHook: libclang for ffmpeg-sys-next's bindings.
            nativeBuildInputs = [ pkgs.pkg-config pkgs.qt6.wrapQtAppsHook pkgs.qt6.qtbase pkgs.rsync pkgs.makeWrapper pkgs.rustPlatform.bindgenHook ];
            # Linked libraries (grove runs no external tools for these).
            buildInputs = qtFor pkgs ++ [ qtEnv pkgs.libGL ] ++ linkedFor pkgs;
            # cxx-qt wants every Qt module under one prefix.
            env.QMAKE = "${qtEnv}/bin/qmake";
            # dmgwiz turns on openssl's "vendored" feature (OpenSSL built from
            # source, which needs perl): link the system openssl instead.
            env.OPENSSL_NO_VENDOR = "1";
            preBuild = ''
              # (Qt's setup hook sets QMAKE to qtbase's alone.)
              export QMAKE=${qtEnv}/bin/qmake
            '';
            buildPhase = persistentCargo "grove" "--release -p grove";
            installPhase = ''
              runHook preInstall
              install -Dm755 target/release/grove $out/bin/grove
              install -Dm644 icons/grove.svg $out/share/icons/hicolor/scalable/apps/grove.svg
              install -Dm644 data/grove.desktop $out/share/applications/grove.desktop
              install -Dm644 data/grove-portal.desktop $out/share/applications/grove-portal.desktop
              install -Dm644 data/grove-player.desktop $out/share/applications/grove-player.desktop
              install -Dm644 data/grove.portal $out/share/xdg-desktop-portal/portals/grove.portal
              install -Dm644 data/mime/grove-tessh.xml $out/share/mime/packages/grove-tessh.xml
              mkdir -p $out/share/dbus-1/services
              for f in data/dbus/*.service; do
                sed "s|@bin@|$out/bin/grove|" "$f" > $out/share/dbus-1/services/$(basename "$f")
              done
              runHook postInstall
            '';
            doCheck = false;
            dontWrapQtApps = true;
            postFixup = ''
              wrapQtApp $out/bin/grove \
                --suffix PATH : ${pkgs.lib.makeBinPath (runtimeFor pkgs)} \
                --set-default GROVE_FONT_DIR ${pkgs.inter}/share/fonts/truetype \
                --suffix XDG_DATA_DIRS : ${pkgs.lib.concatMapStringsSep ":" (p: "${p}/share") [ pkgs.ffmpegthumbnailer pkgs.libheif pkgs.whitesur-icon-theme ]}
            '';
            meta = {
              description = "A Finder-style file manager with Dolphin's power";
              homepage = "https://github.com/benwbooth/grove";
              license = pkgs.lib.licenses.mit;
              mainProgram = "grove";
              platforms = pkgs.lib.platforms.linux;
            };
          };
        in
        { default = grove; inherit grove; });

      apps = forAll (pkgs: {
        default = { type = "app"; program = "${self.packages.${pkgs.system}.default}/bin/grove"; };
      });

      # `nix develop`: Rust and the same Qt; plain `cargo` then uses the
      # host's kache setup (~/.cargo/config.toml).
      devShells = forAll (pkgs:
        let qtEnv = qtEnvFor pkgs; in
        {
          default = pkgs.mkShell {
            packages = [ pkgs.cargo pkgs.rustc pkgs.clippy pkgs.rustfmt pkgs.pkg-config qtEnv pkgs.libGL pkgs.git pkgs.imagemagick pkgs.xvfb-run pkgs.rustPlatform.bindgenHook ] ++ runtimeFor pkgs;
            buildInputs = linkedFor pkgs;
            QMAKE = "${qtEnv}/bin/qmake";
            # Link with this shell's compiler (a system clang links against the
            # system glibc and the binary can't load this nixpkgs' Qt).
            "CARGO_TARGET_${pkgs.stdenv.hostPlatform.rust.cargoEnvVarTarget}_LINKER" = "cc";
            shellHook = ''
              ln -sfn ${qtEnv} .qt-env
              export NIX_LDFLAGS="''${NIX_LDFLAGS//"-rpath $out/lib"/}"
              export GROVE_FONT_DIR=''${GROVE_FONT_DIR:-${pkgs.inter}/share/fonts/truetype}
              export XDG_DATA_DIRS="${pkgs.ffmpegthumbnailer}/share:${pkgs.libheif}/share:${pkgs.whitesur-icon-theme}/share:''${XDG_DATA_DIRS:-}"
            '';
          };
        });
    };
}
