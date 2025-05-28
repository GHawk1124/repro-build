{
  description = "Reproducible Rust cross-build for repro_build";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachSystem [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ] (system:
      let
        overlays = [ rust-overlay.overlays.default ];
        pkgs = import nixpkgs { inherit system overlays; };
        lib = pkgs.lib;

        # Cross-package-sets
        pkgsCrossAarch64 = pkgs.pkgsCross.aarch64-multiplatform;
        pkgsCrossWindows = pkgs.pkgsCross.mingwW64;
        pkgsCrossWindowsStatic = import nixpkgs {
          inherit system overlays;
          crossSystem = {
            config = "x86_64-w64-mingw32";
            libc = "msvcrt";
            platform = { useAndroidPrebuilt = false; };
          };
        };
        pkgsStatic = import nixpkgs {
          inherit system overlays;
          crossSystem = { config = "x86_64-unknown-linux-musl"; };
        };
        pkgsAarch64Static = import nixpkgs {
          inherit system overlays;
          crossSystem = { config = "aarch64-unknown-linux-musl"; };
        };

        # Generic builder for GNU / Musl / MinGW
        buildFor = { targetSystem, targetTriple, needsWine ? false
          , staticBuild ? false }:
          let
            # Map gcc -dumpmachine style to pkgs
            nixTargetSystem = 
              if targetTriple == "x86_64-unknown-linux-musl" then "x86_64-linux"
              else if targetTriple == "aarch64-unknown-linux-musl" then "aarch64-linux"
              else if targetTriple == "x86_64-pc-windows-gnu" then "x86_64-windows"
              else if targetTriple == "aarch64-pc-windows-gnu" then "aarch64-windows"
              else targetSystem; # Default to what was passed for -linux-gnu

            targetPkgs = # This section determines the Nix package set based on the Rust targetTriple and staticBuild
              if staticBuild && targetTriple == "x86_64-unknown-linux-gnu" then pkgsStatic # x86_64-linux-musl
              else if staticBuild && targetTriple == "aarch64-unknown-linux-gnu" then pkgsAarch64Static # aarch64-linux-musl
              else if targetTriple == "x86_64-pc-windows-gnu" then pkgsCrossWindows # x86_64-w64-mingw32 (dynamic, static is handled by pkgsCrossWindowsStatic in RUSTFLAGS)
              else if targetTriple == "aarch64-pc-windows-gnu" then pkgs.pkgsCross.aarch64-multiplatform-windows # Placeholder, assuming a similar structure for ARM windows
              else pkgs; # for x86_64-linux-gnu, aarch64-linux-gnu

            actualTriple = # This is the Rust triple used by rust-bin and CARGO_BUILD_TARGET
              if staticBuild && targetTriple == "x86_64-unknown-linux-gnu" then "x86_64-unknown-linux-musl"
              else if staticBuild && targetTriple == "aarch64-unknown-linux-gnu" then "aarch64-unknown-linux-musl"
              # For windows-gnu, static is handled via RUSTFLAGS, so actualTriple remains x86_64-pc-windows-gnu
              else targetTriple;

            rustBin = pkgs.rust-bin.stable.latest.default.override {
              targets = [ actualTriple ];
            };

            opensslLib = if staticBuild then
              targetPkgs.openssl.override { static = true; }
            else
              pkgs.openssl;

            windowsLibs = if targetTriple == "x86_64-pc-windows-gnu" then [
              pkgsCrossWindows.windows.pthreads
              pkgsCrossWindows.openssl
              # Add zlib which is often needed
              pkgsCrossWindows.zlib
            ] else
              [ ];

            # Improve Windows build configuration
            buildInputs = (if targetTriple == "x86_64-pc-windows-gnu" then
              windowsLibs
            else
              [ opensslLib ]);
            nativeBuildInputs = [ pkgs.pkg-config ]
              ++ (if needsWine then [ pkgs.wine ] else [ ])
              ++ (if targetTriple == "aarch64-unknown-linux-gnu" then
                [ pkgs.qemu ]
              else
                [ ]);

            rustFlags =
              if staticBuild && (actualTriple == "x86_64-unknown-linux-musl" || actualTriple == "aarch64-unknown-linux-musl") then "-C target-feature=+crt-static"
              else if targetTriple == "x86_64-pc-windows-gnu" && staticBuild then "-C target-feature=+crt-static -C linker=${pkgsCrossWindowsStatic.stdenv.cc.targetPrefix}gcc -C link-args=-static"
              else if targetTriple == "x86_64-pc-windows-gnu" then "-C linker=${pkgsCrossWindows.stdenv.cc.targetPrefix}gcc"
              else "";

            extraEnv = builtins.listToAttrs (lib.concatLists [
              (if targetTriple == "x86_64-unknown-linux-gnu"
              && staticBuild then [{
                name = "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER";
                value = "${targetPkgs.stdenv.cc.targetPrefix}cc";
              }] else
                [ ])
              (if targetTriple == "aarch64-unknown-linux-gnu"
              && staticBuild then [
                {
                  name = "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER";
                  value = "${targetPkgs.stdenv.cc.targetPrefix}cc";
                }
                {
                  name = "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUNNER";
                  value = "qemu-aarch64";
                }
              ] else
                [ ])
              (if targetTriple == "x86_64-pc-windows-gnu" then [{
                name = "CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER";
                value = "${
                    (if staticBuild then
                      pkgsCrossWindowsStatic
                    else
                      pkgsCrossWindows).stdenv.cc.targetPrefix
                  }gcc";
              }] else
                [ ])
            ]);

          in targetPkgs.rustPlatform.buildRustPackage rec {
            pname = "repro_build";
            version = "0.1.0";
            src = pkgs.lib.cleanSourceWith {
              src = ../.;
              filter = path: type:
                let baseName = baseNameOf path; in
                  (type == "directory" && baseName != "target" && baseName != ".git" && baseName != "result" && baseName != ".repro-build") ||
                  (type == "directory" && baseName == "templates") ||
                  (type == "regular" && (
                    pkgs.lib.hasSuffix ".rs" baseName ||
                    pkgs.lib.hasSuffix ".toml" baseName ||
                    pkgs.lib.hasSuffix ".lock" baseName ||
                    pkgs.lib.hasSuffix ".md" baseName ||
                    pkgs.lib.hasSuffix ".hbs" baseName ||
                    baseName == "LICENSE" ||
                    baseName == ".gitignore"
                  ));
            };
            cargoLock = { lockFile = ../Cargo.lock; };
            release = true;

            # Targeted build
            CARGO_BUILD_TARGET = actualTriple;

            inherit buildInputs nativeBuildInputs;

            # Optimize
            CARGO_PROFILE_RELEASE_LTO = "true";
            CARGO_PROFILE_RELEASE_OPT_LEVEL = "s";
            CARGO_PROFILE_RELEASE_CODEGEN_UNITS = "1";
            CARGO_PROFILE_RELEASE_PANIC = "abort";
            CARGO_PROFILE_RELEASE_STRIP = "true";

            # Static if requested
            RUSTFLAGS = rustFlags;

            # Windows-specific install phase to handle .exe files
            installPhase = if (targetTriple == "x86_64-pc-windows-gnu" || targetTriple == "aarch64-pc-windows-gnu") then ''
              mkdir -p $out/bin
              
              # More verbose debugging
              echo "Contents of target directory:"
              find target -type d | sort
              
              echo "Looking for .exe files:"
              find target -name "*.exe" || echo "No .exe files found"
              
              # Try multiple possible locations
              if [ -f "target/${actualTriple}/release/repro_build.exe" ]; then
                echo "Found .exe at expected location"
                cp target/${actualTriple}/release/repro_build.exe $out/bin/
              elif [ -f "target/release/repro_build.exe" ]; then
                echo "Found .exe in target/release"
                cp target/release/repro_build.exe $out/bin/
              else
                echo "Searching for repro_build.exe in all locations"
                find target -name "repro_build.exe" -exec cp {} $out/bin/ \; || echo "No repro_build.exe found anywhere"
                
                echo "Copying any .exe files found as fallback"
                find target -name "*.exe" -exec cp {} $out/bin/ \; || echo "No .exe files found at all"
              fi
              
              # Check what was actually installed
              echo "Contents of $out/bin:"
              ls -la $out/bin/
            '' else null;

            # Extra vars
            passthru = extraEnv;
          };

      in {
        # Native builds with new naming scheme
        packages."x86_64-linux-gnu" = buildFor {
          targetSystem = "x86_64-linux";
          targetTriple = "x86_64-unknown-linux-gnu";
        };
        packages."aarch64-linux-gnu" = buildFor {
          targetSystem = "aarch64-linux";
          targetTriple = "aarch64-unknown-linux-gnu";
        };

        # Static musl builds
        packages."x86_64-linux-musl" = buildFor {
          targetSystem = "x86_64-linux"; # Original system for pkgs derivation
          targetTriple = "x86_64-unknown-linux-gnu"; # Rust GNU triple that gets converted to musl
          staticBuild = true;
        };
        packages."aarch64-linux-musl" = buildFor {
          targetSystem = "aarch64-linux";
          targetTriple = "aarch64-unknown-linux-gnu";
          staticBuild = true;
        };

        # Windows builds (GNU default)
        packages."x86_64-w64-mingw32" = buildFor {
          targetSystem = "x86_64-windows"; # Nix system string
          targetTriple = "x86_64-pc-windows-gnu"; # Rust triple
          needsWine = true;
        };
        
        # Add a static MinGW build option
        packages."x86_64-w64-mingw32-static" = buildFor {
          targetSystem = "x86_64-windows";
          targetTriple = "x86_64-pc-windows-gnu";
          needsWine = true;
          staticBuild = true; 
        };
        packages."aarch64-w64-mingw32" = buildFor {
          targetSystem = "aarch64-windows";
          targetTriple = "aarch64-pc-windows-gnu"; # Assuming this is the Rust triple for ARM Windows GNU
          needsWine = true; # May need QEMU as well or instead depending on host
        };

        # Windows MSVC builds (uses pkgs.stdenv.mkDerivation directly, not buildFor)
        packages."x86_64-pc-windows-msvc" = pkgs.stdenv.mkDerivation {
          pname = "repro_build-msvc";
          version = "0.1.0";
          src = pkgs.lib.cleanSourceWith {
            src = ../.;
            filter = path: type:
              let baseName = baseNameOf path; in
                (type == "directory" && baseName != "target" && baseName != ".git" && baseName != "result" && baseName != ".repro-build") ||
                (type == "directory" && baseName == "templates") ||
                (type == "regular" && (
                  pkgs.lib.hasSuffix ".rs" baseName ||
                  pkgs.lib.hasSuffix ".toml" baseName ||
                  pkgs.lib.hasSuffix ".lock" baseName ||
                  pkgs.lib.hasSuffix ".md" baseName ||
                  pkgs.lib.hasSuffix ".hbs" baseName ||
                  baseName == "LICENSE" ||
                  baseName == ".gitignore"
                ));
          };

          nativeBuildInputs = [
            (pkgs.rust-bin.stable.latest.default.override {
              targets = [ "x86_64-pc-windows-msvc" ];
            })
            pkgs.cargo-xwin
            pkgs.clang
            pkgs.llvmPackages.lld
            pkgs.wine
            pkgs.pkg-config
            pkgs.openssl
          ];

          buildPhase = ''
            export CARGO_HOME=$PWD/.cargo
            mkdir -p $CARGO_HOME/registry $CARGO_HOME/git
            export SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt
            export NIX_SSL_CERT_FILE=$SSL_CERT_FILE
            export XWIN_ACCEPT_LICENSE=yes
            export XWIN_CACHE_DIR=$PWD/.cache/xwin
            cargo xwin build \
                --release \
                --locked \
                --target x86_64-pc-windows-msvc
          '';

          installPhase = ''
            mkdir -p $out/bin
            find target -type f -executable -name "*.exe" || echo "No executables found"
            cp target/x86_64-pc-windows-msvc/release/repro_build{,.exe} $out/bin/ || true

            # If it's a library, install that instead
            mkdir -p $out/lib
            find target -name "*.dll" -o -name "*.lib" -o -name "*.a" || echo "No libraries found"
            cp target/x86_64-pc-windows-msvc/release/*.{dll,lib} $out/lib/ 2>/dev/null || true
          '';
        };

        # Default package points to the host's gcc -dumpmachine style triple
        default = self.packages."${pkgs.stdenv.hostPlatform.config}";

        # ——— Dev-Shells ———
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = [ pkgs.rust-bin.stable.latest.default ];
          buildInputs = [ pkgs.openssl pkgs.pkg-config ];
        };

        devShells."aarch64-linux-gnu" = pkgs.mkShell {
          nativeBuildInputs = [
            (rust-overlay.lib.mkRustBin { }
              pkgsCrossAarch64.buildPackages.stable.latest.default.override {
                targets = [ "aarch64-unknown-linux-gnu" ];
              })
            pkgs.pkg-config
            pkgs.qemu
          ];
          buildInputs = [ pkgs.openssl ];
          shellHook = ''
            export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=${pkgsCrossAarch64.stdenv.cc.targetPrefix}cc
            export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER=qemu-aarch64
          '';
        };

        devShells."x86_64-linux-musl" = pkgs.mkShell {
          nativeBuildInputs = [
            (pkgsStatic.rust-bin.stable.latest.default.override {
              targets = [ "x86_64-unknown-linux-musl" ];
            })
            pkgs.pkg-config
          ];
          buildInputs = [ (pkgsStatic.openssl.override { static = true; }) ];
          shellHook = ''
            export RUSTFLAGS="-C target-feature=+crt-static"
            export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=${pkgsStatic.stdenv.cc.targetPrefix}cc
          '';
        };

        devShells."x86_64-w64-mingw32" = pkgs.mkShell {
          nativeBuildInputs = [
            (pkgs.rust-bin.stable.latest.default.override {
              targets = [ "x86_64-pc-windows-gnu" ];
            })
            pkgs.pkg-config
            pkgs.wine
          ];
          buildInputs = [
            pkgsCrossWindowsStatic.windows.pthreads
            pkgsCrossWindowsStatic.openssl
          ];
          shellHook = ''
            export RUSTFLAGS="-C target-feature=+crt-static -C linker=x86_64-w64-mingw32-gcc -C link-args=-static"
            export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=${pkgsCrossWindowsStatic.stdenv.cc.targetPrefix}gcc
          '';
        };

        devShells."x86_64-pc-windows-msvc" = pkgs.mkShell {
          nativeBuildInputs = [
            (pkgs.rust-bin.stable.latest.default.override {
              targets = [ "x86_64-pc-windows-msvc" ];
            })
            pkgs.cargo-xwin
            pkgs.clang
            pkgs.llvmPackages.lld
            pkgs.wine
            pkgs.pkg-config
          ];
          shellHook = ''
            export XWIN_ACCEPT_LICENSE=yes
            export XWIN_CACHE_DIR=$PWD/.cache/xwin
            echo "Run → cargo xwin build --target x86_64-pc-windows-msvc"
          '';
        };
      });
}
