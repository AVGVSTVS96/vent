# vent

Fan control and temperatures for Apple Silicon Macs, from the command line.

```console
$ vent
left    1781 rpm  auto
right   1870 rpm  auto
cpu 87°  gpu 65°  memory 59°  ssd 40°  battery 32°
heat 75°, auto curve 70° to 95°
```

## Install

```sh
curl -LsSf https://github.com/AVGVSTVS96/vent/releases/latest/download/vent-installer.sh | sh
sudo vent install
```

`sudo vent install` sets up the small root daemon that runs the fans. Run it again after updating vent.

## How it runs the fans

Every fan starts on auto. While the chip is cool, macOS runs the fans, which on Apple Silicon means quiet or off. Once the chip's heat, the hotter of the CPU's and GPU's average sensor, passes the curve's start, vent takes every auto fan and runs it from its minimum there to its maximum at the curve's end. When the heat drops 3° below the start, macOS gets them back.

The heat is smoothed over about ten seconds, so a short burst of work doesn't spin the fans up. The daemon checks every second, and if anything else takes a fan, it takes it back.

## Use

```sh
vent                   # fans, temperatures and the heat the curve sees
vent curve             # the curve auto fans follow
vent curve 60 85       # start at 60°, full speed at 85°
vent set right max     # left, right, an index, or all
vent set left 2500     # rpm
vent set all 60%       # percent of each fan's range
vent set left system   # always macOS, never the curve
vent auto              # every fan back on auto
vent smc list T        # raw SMC keys by prefix
sudo vent uninstall    # give every fan back to macOS for good
```

Settings hold across reboots. The daemon gives every fan to macOS before the Mac sleeps, when it stops, and if it crashes, and picks them up again on wake and start.

Tested on an M1 Max MacBook Pro.
