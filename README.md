<p align="center">
  <img src="https://github.com/flatkvm/flatkvm/blob/master/flatkvm.png?raw=true" height="150" width="150" alt="Flatkvm logo"/>
</p>

Flatkvm is a tool to easily run [flatpak](https://flatpak.org/) apps isolated inside a VM, using QEMU/KVM.

**WARNING**: This is beta quality software and hasn't been externally audited, use it with care. If you need trusted isolation, take a look at [Qubes OS](https://www.qubes-os.org/).

# How does it work?

The **flatkvm** binary on the Host executes a QEMU instance accelerated by KVM, using an snapshot of a specialized template as root filesystem, and a per-app virtual disk image which is automatically created on demand. The VM starts an Xorg session running [i3wm](https://i3wm.org/) and [flatkvm-agent](https://github.com/flatkvm/flatkvm-agent), a small program which communicates with the binary on the Host using a **virtio-vsock** device.

Once the session in the VM is ready, **flatkvm** instructs the **agent** to mount the flatpak directories, shared read-only from the Host using **virtio-9p**, and to execute the flatpak application indicated in by the user in the command line.

Afterward, **flatkvm** and **agent** keep the communication open to notify about clipboard events, D-Bus notifications, and to signal the eventual termination of the flatpak application. Once the app has exited, **flatkvm** sends an ACPI shutdown signal to the VM using the **QMP** interface.

# Demo

[![Video](https://img.youtube.com/vi/K_FizklyrKs/maxresdefault.jpg)](https://youtu.be/K_FizklyrKs)

# Known issues
 
 - **If enabled, all clipboard contents of the Host are shared with the VM**: Automatic clipboard sharing implies that all contents of the Host are sent to the VM, and viceversa, which is a significant breach in the isolation. Ideally, clipboard sharing should be on-demand, probably provided by some D-Bus service, and integrated in the UI.
   - On version **0.1.5**, flatpak implements the *discrete* clipboard mode, which can be selected by passing the *--clipboard discrete* argument on the command line. In this mode, the flatkvm no longer sends all Host's clipboard updates to the VM. Instead, clipboard data must explicitly shared using [flatkvm-paste](https://github.com/flatkvm/flatkvm-paste).
 
 - **Two-finger/wheel scrolling doesn't work**: QEMU's GTK UI seems to have trouble receiving scrolling events on Wayland. The workaround is switching to **GNOME on Xorg**. This is being tracked in [#3](https://github.com/flatkvm/flatkvm/issues/3).
 
 - **Spotify starts up but the VM is immediately shut down**: Spotify (and probably some other apps too) fork+exec's after running, which confuses the **flatkvm-agent** into thinking the app has already exited. The workaround is passing the **-n** flag to **flatkvm run** to disable automatic shut down.
 
  - **The first run of Steam takes a long time**: This is the result of combination of **virtio-9p**'s poor performance and Steam insisting of inspecting each library present in the package. After Steam has updated itself, its runtime (and the games) will reside in the dedicated virtual disk, so this will no longer be an issue.
  
  - **VirtualBox can't start any VM when there's a Flatkvm application running**: Flatkvm uses KVM, which implies loading and initializing **kvm.ko**. As multiple virtualization technologies can't be simultaneously enabled on the same machine, VirtualBox can't be used in parallel with Flatkvm. The only possible workaround is switching from VirtualBox to **virt-manager**.
  
  - **KeepassXC can't save nor open any database**: KeepassXC, and possibly other apps, doesn't play nice with xdg-desktop-portal file access model. This is being tracked in [#2](https://github.com/flatkvm/flatkvm/issues/2).

 
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

Despite its young age, flatkvm is already quite complex with four different components ([flatkvm](https://github.com/flatkvm/flatkvm), [flatkvm-agent](https://github.com/flatkvm/flatkvm-agent), [flatkvm-linux](https://github.com/flatkvm/flatkvm-linux) and [flatkvm-template-debian](https://github.com/flatkvm/flatkvm-template-debian)) interacting between them. Being honest, porting, packaging and testing it for every distro requires an amount of time I don't currently have, so I'm focusing on Fedora.

Or course, if you want to volunteer to port and maintain flatkvm for another distro, you'll be more than welcome! ;-)
