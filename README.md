# One Key PCMidi

## Usage

1. `cargo build --release`
2. `one_key_pcmidi <input_file>`
   - Input format: *48000Hz 16bit PCM Wave file*
3. Play in real-time using [Kiva](https://github.com/arduano/Kiva), or convert to audio by Keppy's MIDI Converter
   - The soundfont in [misc](misc/) folder is *required*