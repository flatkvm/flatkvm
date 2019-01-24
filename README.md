Flatkvm is a tool to easily run [flatpak](https://flatpak.org/) apps isolated inside a VM, using QEMU/KVM.

**WARNING**: This is alpha quality software and it'll probably crash and burn in many ways. Use it with care and avoid depending on it for sensitive workloads. If you need trusted isolation, take a look at [Qubes OS](https://www.qubes-os.org/).

# How does it work?

The **flatkvm** binary on the Host executes a QEMU instance accelerated by KVM, using an snapshot of a specialized template as root filesystem. On this template, a Xorg session is started, running [i3wm](https://i3wm.org/) and [flatkvm-agent](https://github.com/flatkvm/flatkvm-agent). The later communicates with the binary on the Host using a **virtio-vsock** device.

Once the session in the VM is ready, **flatkvm** instructs the **agent** to mount the flatpak directories, shared from the Host using **virtio-9p** (all volumes, except the one for storing the application's data, are shared read-only), and to execute the flatpak application instructed by the user.

Afterward, **flatkvm** and **agent** keep the communication open to notify about clipboard events, D-Bus notifications, and to signal the eventual termination of the flatpak application. Once the flatpak application has exited, **flatkvm** sends an ACPI shutdown signal to the VM using the **QMP** interface.

# Known issues

 - **No sound in FirefoxNightly nor FirefoxDevEdition**: For some reason yet to be determined, the template based on Alpine Linux is not compatible with cubeb sandboxing. For the moment, simply disable the **media.cubeb.sandbox** option in **about:config**, and restart the app.
 
 - **If enabled, all clipboard contents of the Host are shared with the VM**: Automatic clipboard sharing implies that all contents of the Host are sent to the VM, and viceversa, which is a significant breach in the isolation. Ideally, clipboard sharing should be on-demand, probably provided by some D-Bus service, and integrated in the UI.
 
 - **Some apps refure to run as root**: Due to a limitation in Linux's 9p filesystem driver, the app inside the VM needs to run as **root** user (note this is just the user **inside** the VM, not the user on the Host). Some flatpak apps (VLC, Steam, and possibly others) refuse running as root. This will be eventually solved with some work on the 9p filesystem driver.
 
 - **Spotify starts up but the VM is immediately shut down**: Spotify (and probably some other apps too) fork+exec's after running, which confuses the **flatkvm-agent** into thinking the app has already existed. The workaround is passing the **-n** flag to **flatkvm run** to disable automatic shut down.
 
# Installing
## Fedora
 
The easiest way to give flatkvm a try is by using [this Copr repository](https://copr.fedorainfracloud.org/coprs/slp/flatkvm/):
 
```
$ sudo dnf copr enable slp/flatkvm

$ sudo dnf install flatkvm
```

The install some app using flatpak:

```
flatpak install --user --from https://firefox-flatpak.mojefedora.cz/org.mozilla.FirefoxNightly.flatpakref
```

And execute it with flatkvm:

```
flatkvm run org.mozilla.FirefoxNightly
```
## Other distros

Despite its young age, flatkvm is already quite complex with four different components ([flatkvm](https://github.com/flatkvm/flatkvm), [flatkvm-agent](https://github.com/flatkvm/flatkvm-agent), [flatkvm-linux](https://github.com/flatkvm/flatkvm-linux) and [flatkvm-template-alpine](https://github.com/flatkvm/flatkvm-template-alpine)) interacting between them. Being honest, porting, packaging and testing it for every distro requires an amount of time I don't currently have, so I'm focusing on Fedora.

Or course, if you want to volunteer to port and maintain flatkvm for another distro, you'll be more than welcome! ;-)
