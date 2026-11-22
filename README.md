<div align="center">

<img src=".github/resources/advance-sp-logo.png" width="560" alt="ADVANCE·SP">

**slot.sp, extended for the couch: Bluetooth game controllers today, HDMI to the TV next.**

**[Download](https://github.com/klauspaschek/advance.sp/releases/latest)** · **[What's new](#what-advancesp-adds)** · **[Controllers](#bluetooth-controllers)** · **[TV](#on-a-tv-hdmi)** · **[Install & update](#install-and-update)** · **[Credits](#credits)**

</div>

**advance.sp** is a launcher for the Anbernic **RG SP**, built on [slot.sp](https://github.com/jgraving/slot.sp) by Jake Graving. It keeps everything slot.sp does: GBA, GB and GBC games as cartridges in an SP-styled carousel, save and resume, light and dark themes, Bluetooth and USB audio. It then adds what is needed to play on the sofa with a controller in hand. The controller and TV support is ported from [Cartridge](https://github.com/NukeBomB3000/cartridge), a sibling fork of the same launcher family.

## What advance.sp adds

| | Status |
| :--- | :--- |
| 🎮 **Bluetooth game controllers**: pair, play, sleep and wake from the controller | ✅ Done, tested on an RG SP with an Xbox Wireless Controller |
| 📶 **A Bluetooth tile** in the menu: one place for the radio, headphones and controllers | ✅ Done |
| 🎧 **Headphones that behave**: their volume buttons and slot's stay in step, headphones that connect by themselves take the sound, and volume never jumps loud out of mute | ✅ Done |
| 📺 **[HDMI out](#on-a-tv-hdmi)** through the RG SP's mini HDMI port: plug in a TV and the picture follows the cable | 🧪 Built, awaiting its first test on an RG SP |

### Menu changes from slot.sp

- **Bluetooth** takes the place of *Parental controls* on the main menu. It holds the Bluetooth switch, every paired device (headphones and controllers, each with its icon) and **Pair new device**.
- **Device controls** gains **Test buttons** (see [below](#bluetooth-controllers)), and its **In-Game** page opens on **Controller layout**: Nintendo or Xbox face buttons for a controller, greyed out until one is connected.
- **General** is regrouped: Updates, then Date & Time, Automatic time, Time zone and Clock format together, then Device code, Parental controls and About.
- **Device code** can be **No code**: Wi-Fi and RetroAchievements credentials are then saved without a code and open by themselves, with no prompt at startup (anyone holding the SD card could read them). With a code, **Startup access → Ask when needed** no longer asks at startup, only when Network, Wi-Fi, achievements or sharing first need it.
- **Sound** keeps volume, the output choice (speaker, wired, Bluetooth or USB headphones) and the disconnect options. Headphones are paired in the Bluetooth tile.
- **Parental controls** (kid mode) are unchanged and now live in **General**.

### Sound and startup changes

- **Headphone volume buttons** now control slot's volume, and slot's volume buttons set the headphones' own level, on headphones that support Bluetooth absolute volume. Muting silences them too, and their buttons work from zero.
- **Coming out of mute**, volume up starts at the lowest step instead of jumping back to the old level. Toggling mute again without touching the volume restores it as it was.
- **Headphones you switch on** take the sound as soon as they connect, without choosing them again in Sound.
- **A new full-screen ADVANCE·SP boot logo**, shrunk to fit on handhelds with smaller screens. Your own `Config/bootlogo.bmp` still takes precedence.

## Bluetooth controllers

1. Open **Menu → Bluetooth** and turn Bluetooth on.
2. Put the controller in pairing mode (on an Xbox controller, hold the small pair button until the logo blinks fast).
3. Choose **Pair new device → Scan for devices**, select the controller and press <kbd>A</kbd>.

The controller reconnects by itself whenever Bluetooth is on, and can join or leave at any time, even mid-game. Pairing supports both classic Bluetooth and Bluetooth LE controllers, and any controller that reports Linux's standard gamepad layout should work. The Xbox Wireless Controller is the one tested so far.

| Controller | On the RG SP |
| :--- | :--- |
| Face buttons | **Nintendo layout** by default: the right button is A and the top one X, as on the handheld. **Device controls → In-Game → Controller layout → Xbox** follows the printed letters instead. |
| D-pad, left stick | D-pad |
| LB / RB, LT / RT | L1 / R1, L2 / R2 |
| View / Menu | Select / Start |
| Xbox button | MENU: tap for the menu, hold to eject the game |
| Share, tap | Screenshot |
| Share, hold | The power button: let go to sleep, keep holding to save and power off |
| L3 / R3 (sticks pressed in) | Save state / load state (both can be undone from the game menu) |
| Right stick ← / → | Screen size: the next smaller / larger (Actual, Stretch, Fit, Integer), in any game |
| Right stick ↑ / ↓ | Volume up / down, one step per push: return the stick to the centre before the next |

Turning the controller on while the RG SP is in light sleep wakes the handheld, as a console does. A deep-sleeping or powered-off RG SP can only be woken by its own power button.

The handheld's own buttons keep working alongside the controller, and any remapping in **Device controls** applies to both.

**Device controls → Test buttons** checks every button: a drawing of the RG SP, or of an Xbox controller once one is connected and pressed, lights up each button while it is held, with the sticks' directions and the names slot receives beneath. Nothing else happens while it is open (the power button and the lid still work). Hold <kbd>B</kbd> for a second to go back.

<details>
<summary><strong>Checking a new controller's buttons</strong></summary>

Put an empty file named `TRACE_INPUT` at the root of the card and restart. advance.sp then records what every input device reports, and every press, in `Logs/input-trace.log`. Remove the file afterwards.

</details>

## On a TV (HDMI)

Connect the RG SP's mini HDMI port to a TV with a mini-HDMI cable or adapter. Plugging the cable in or pulling it out works at any time, even mid-game: advance.sp saves the game, switches the picture to the other screen within a few seconds and carries on in the same game. It switches only after the cable has been in, or out, for two seconds, so wiggling the plug does nothing. If you are in a link session, it waits until the session ends.

| On the TV | |
| :--- | :--- |
| Picture | The TV receives 1080p at 60 Hz. The handheld's 3:2 picture fills the TV's height and is centred, with black bars at the sides. The handheld's own screen is off. |
| Sound | Plays through the TV, unless Bluetooth or USB headphones already have it. **Sound** gains a **TV (HDMI)** output, so you can switch between the TV and headphones. Pulling the cable gives the sound back to the handheld. |
| Lid | Closing it does not put the handheld to sleep, so you can fold it away and play with a controller. |

Without a cable nothing changes.

<details>
<summary><strong>If the TV stays dark</strong></summary>

Every start with a cable in adds a line such as `hdmi: cable in, output hdmi, fb 720,960, sound 'plughw:CARD=ahubhdmi,DEV=0'` to `Logs/slot-session.log`. If the switch failed, the log says why. Please include that file, and `Logs/slot.log`, when you report a problem.

</details>

## Install and update

advance.sp installs exactly like slot.sp and uses the same folders. Follow slot.sp's [new installation](https://jakegraving.com/slot.sp/installing/) guide, but take the downloads from **[this repository's releases](https://github.com/klauspaschek/advance.sp/releases/latest)**:

- **slot.sp-System.zip** updates an existing slot.sp or advance.sp card: replace the `System` folder on the content card (SD2).
- **slot.sp-SD2.zip** is the full starter tree for a new card.

Coming from slot.sp, replacing `System` is all it takes. Games, saves, labels, wallpapers and settings live outside `System` and stay as they are. The in-app updater (**General → Updates**) installs advance.sp releases from this repository from then on, and also replaces only `System`.

## Everything else

All of slot.sp's features and guides apply: [playing](PLAYING.md), [buttons & shortcuts](CONTROLS.md), [appearance](DISPLAY.md), [library & artwork](LIBRARY.md), [settings](SETTINGS.md), [sleep & power](POWER-OPTIONS.md), [RetroAchievements](RETROACHIEVEMENTS.md) and more in the [guide index](GUIDE.md). Where they describe pairing headphones in Sound or the Parental controls tile, see [menu changes](#menu-changes-from-slotsp) above.

Built for the RG SP. Other handhelds slot.sp supports should run it too, but have not been tested.

## Credits

- **[slot.sp](https://github.com/jgraving/slot.sp)** by **Jake Graving**, which advance.sp extends, based on [slot](https://github.com/BrandonKowalski/slot) by **Brandon T. Kowalski**, with original artwork by [MarkusMaribu](https://github.com/MarkusMaribu).
- **[Cartridge](https://github.com/NukeBomB3000/cartridge)** by NukeBomB3000, a fork of [omega-zero](https://github.com/oauramos/omega-zero) by Otávio ([@oauramos](https://github.com/oauramos)), for the external-controller input layer and HDMI output this port is based on.
- **[BaseOS](https://github.com/pvaibhav/BaseOS)** by pvaibhav, the system underneath; the way to switch an H700 to HDMI was found in pvaibhav's NextUI port.

advance.sp's additions were developed with the help of an AI assistant (Claude) and tested on the maintainer's RG SP.

Not affiliated with Nintendo or Anbernic. No games or BIOS files are included.

[GPL-3.0-or-later](LICENSE) · [slot.sp's full credits](https://jakegraving.com/slot.sp/credits/)
