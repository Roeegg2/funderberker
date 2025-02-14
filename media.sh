#! /bin/sh
# basic script to write Funderberker to a bootable media

DEVICE=""

lsblk
read -p "Enter device (in the format /dev/<...>):" DEVICE
while true; do
  read -p "WARNING - everything on '$DEVICE' WILL be deleted. Are you OK with that? (y/n)" yn
  case $yn in
    [Yy]* ) break;;
    [Nn]* ) echo "Exiting..."; exit;;
    * ) echo "Please answer yes or no.";;
  esac
done  

echo "Formatting $DEVICE to FAT..."
sudo mkfs.fat -F 32 $DEVICE

echo "Creating dir 'media' to mount '$DEVICE' on..."
mkdir -p ./media

echo "Mounting '$DEVICE'..."
sudo mount $DEVICE ./media || exit

echo "Setting up ESP dirs..."
mkdir -p media/EFI || exit
mkdir -p media/EFI/BOOT || exit

echo "Copying ESP to 'media'..."
sudo cp esp/efi/boot/* ./media/EFI/BOOT || exit

echo "Done!"
