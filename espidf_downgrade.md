Here is a simple summary of how to remove ESP-IDF 6.0 and install 5.3.1:

1. **Remove the Old Python Environment (if needed):**
   - Delete the old virtual environment to avoid conflicts:
     ```bash
     rm -rf ~/.espressif/python_env/idf6.0_py3.12_env
     ```
   - (Optional) Unset the environment variable if you previously set it:
     ```bash
     unset IDF_PYTHON_ENV_PATH
     ```

2. **Switch to the ESP-IDF 5.3.1 Version:**
   - Navigate to your ESP-IDF folder:
     ```bash
     cd ~/esp/esp-idf
     ```
   - Check out the 5.3.1 release:
     ```bash
     git fetch --all --tags --prune
     git checkout v5.3.1
     git submodule update --init --recursive
     ```
   - *(Alternatively, you can clone a fresh copy targeting v5.3.1: `git clone -b v5.3.1 --recursive https://github.com/espressif/esp-idf.git esp-idf-v5.3.1`)*[1].

3. **Install Required Tools:**
   - Run the install script:
     ```bash
     ./install.sh
     ```
   - This will install the correct tools and Python packages for ESP-IDF 5.3.1.

4. **Activate the Environment:**
   - Source the export script to set up your environment:
     ```bash
     . ~/esp/esp-idf/export.sh
     ```

5. **Verify the Installation:**
   - You can now build your projects with `idf.py build`.

This process ensures a clean switch from ESP-IDF 6.0 to 5.3.1, with all dependencies managed correctly[1][2].

[1] https://github.com/espressif/esp-idf/releases
[2] https://docs.espressif.com/projects/esp-idf/en/stable/esp32/get-started/index.html
[3] https://www.reddit.com/r/esp32/comments/17qqcc3/request_helpdowngrading_esp_idf/
[4] https://github.com/espressif/esp-idf/issues/11102
[5] https://esp32.com/viewtopic.php?t=41132
[6] https://docs.espressif.com/projects/esp-idf/en/v5.2.5/esp32/esp-idf-en-v5.2.5-esp32.pdf
[7] https://docs.espressif.com/projects/esp-idf/en/v5.3/esp32/esp-idf-en-v5.3-esp32.pdf
[8] https://docs.espressif.com/projects/esp-idf/en/stable/esp32/get-started/windows-setup.html
[9] https://docs.espressif.com/projects/esp-idf/en/stable/esp32/versions.html
[10] https://github.com/espressif/vscode-esp-idf-extension/issues/1378