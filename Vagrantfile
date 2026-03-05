Vagrant.configure("2") do |config|
  config.vm.box = "ubuntu/jammy64"
  config.vm.hostname = "enter-sandbox-p2"
  config.vm.synced_folder ".", "/workspace"

  config.vm.provider "virtualbox" do |vb|
    vb.name = "enter-sandbox-p2"
    vb.cpus = 4
    vb.memory = 8192
  end

  config.vm.provision "shell", inline: <<-SHELL
    set -euxo pipefail
    apt-get update
    apt-get install -y build-essential clang pkg-config git curl python3 python3-venv python3-pip
    if [ ! -x /home/vagrant/.cargo/bin/rustup ]; then
      su - vagrant -c "curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal"
    fi
    su - vagrant -c "/home/vagrant/.cargo/bin/rustup target add wasm32-wasip1"
    su - vagrant -c "cd /workspace && python3 scripts/prepare_cpython_wasi_assets.py"
  SHELL
end
