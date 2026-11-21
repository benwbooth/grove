# Installing grove on NixOS

In `/etc/nixos/flake.nix`:

```nix
inputs.grove.url = "github:benwbooth/grove";
# (or "git+file:///home/ben/src/grove?ref=main" to build the local checkout)
```

In the system configuration (with `inputs` passed to the module):

```nix
environment.systemPackages = [ inputs.grove.packages.${pkgs.system}.default ];

# grove as the file chooser for every app under arbor (the existing
# xdg.portal.config.arbor block is set with lib.mkForce: add this line to it)
xdg.portal.config.arbor."org.freedesktop.impl.portal.FileChooser" = [ "grove" ];
# The package ships share/xdg-desktop-portal/portals/grove.portal; make sure
# the portal finds it:
xdg.portal.extraPortals = [ inputs.grove.packages.${pkgs.system}.default ];

# Folders open in grove everywhere (also settable from grove's Settings):
xdg.mime.defaultApplications."inode/directory" = "grove.desktop";
```

D-Bus activation: the package installs `org.freedesktop.FileManager1.service`
(and grove's own and the portal's). If Dolphin is also installed, whichever
service file wins in the system profile answers "Show in folder" when no
file manager is running; a running grove takes the name if it's free.
`services.dbus.packages = [ grove ];` is not needed: systemPackages share
`share/dbus-1/services`.
