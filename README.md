# vent

Fan control and temperatures for Apple Silicon Macs, from the command line.

```console
$ vent
left    1503 rpm  min
right   5810 rpm  max
cpu 69°  gpu 58°  memory 50°  ssd 49°  battery 44°
```

## Install

```sh
curl -LsSf https://github.com/AVGVSTVS96/vent/releases/latest/download/vent-installer.sh | sh
sudo vent install
```

`sudo vent install` sets up the small root daemon that writes fan speeds. Run it again after updating vent.

## Use

```sh
vent                   # fans and temperatures
vent set right max     # left, right, an index, or all
vent set left 2500     # rpm
vent set all 60%       # percent of each fan's range
vent auto              # give every fan back to macOS
vent smc list T        # raw SMC keys by prefix
sudo vent uninstall
```

A setting holds until you change it, across reboots. The daemon gives every fan back to macOS before the Mac sleeps, when it stops, and if it crashes, and takes them again on wake and start.

Run one fan controller at a time: another app writing fan speeds will undo vent's.
