{ pkgs, lib, config, inputs, ... }:

let
  # Package the pre-built binary (build with: bun build src/main.ts --compile --outfile pylon)
  pylon-bin = pkgs.runCommand "pylon-bin" {} ''
    mkdir -p $out/bin
    cp ${./pylon} $out/bin/pylon
    chmod +x $out/bin/pylon
  '';

  pylon-runtime = pkgs.buildEnv {
    name = "pylon-runtime";
    paths = [ pylon-bin pkgs.cacert pkgs.sqlite pkgs.bash ];
    pathsToLink = [ "/bin" "/etc" ];
  };
in
{
  languages.typescript.enable = true;

  packages = with pkgs; [
    bun
    buf
    sqlite
    cacert
    inputs.nix2container.packages.${pkgs.system}.skopeo-nix2container
  ];

  processes = {
    pylon.exec = "bun run start";
  };

  containers.pylon = {
    name = "pylon";
    copyToRoot = [ pylon-runtime ];
    startupCommand = "PATH=${pylon-runtime}/bin:$PATH ${pylon-runtime}/bin/pylon";
  };

  tasks = {
    "ci:build" = {
      exec = "bun install && bun build src/main.ts --compile --outfile pylon";
    };
    "ci:typecheck" = {
      exec = "bun run typecheck 2>/dev/null || bunx tsc --noEmit";
    };
    "container:build" = {
      exec = "devenv container build pylon";
    };
    "container:copy" = {
      exec = ''
        IMAGE=$(devenv container build pylon 2>&1 | tail -1)
        nix run github:nlewo/nix2container#skopeo-nix2container -- copy nix:$IMAGE containers-storage:pylon:latest
        echo "Container copied to podman: pylon:latest"
      '';
    };
    "container:run" = {
      exec = "podman run --rm -d --name pylon -p 15316:15316 -e PYLON_DB_PATH=/data/pylon.db -v pylon-data:/data pylon:latest";
    };
    "container:stop" = {
      exec = "podman stop pylon && podman rm pylon";
    };
  };

  enterShell = ''
    echo "Pylon Development Environment"
    echo "Commands: bun run start | bun run typecheck"
    echo ""
    echo "Container commands:"
    echo "  devenv task container:build   - Build OCI container"
    echo "  devenv task container:copy    - Copy to podman"
    echo "  devenv task container:run     - Run container (port 15316)"
  '';
}
