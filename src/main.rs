
use std::io::Write;
use std::path::Path;
use std::{env, fs::File};
use apres::{self, MIDIBytes};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::conv::ConvertibleSample;
use symphonia::core::formats::FormatOptions;
use symphonia::core::audio::{self, SignalSpec, SampleBuffer};
use symphonia::core::io::{self, MediaSource, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::default;
use apres::MIDIEvent;
use std::f32;



fn load_media_to_pcm<T: ConvertibleSample>(input_media: Box<dyn MediaSource>, pcm_vec: &mut Vec<Vec<T>>) -> Result<u32, Box<dyn std::error::Error>> {
    let codec = default::get_codecs();
    let probe = default::get_probe();
    let mss = io::MediaSourceStream::new(input_media, MediaSourceStreamOptions::default());
    // read the file and convert it into PCM
    let mut format = probe.format(&Hint::default(), mss, &FormatOptions::default(), &MetadataOptions::default())
        .unwrap().format;
    //let format = b_format.as_mut();
    let track = &format.tracks()[0];

    let samplerate = track.codec_params.sample_rate.expect("no samplerate");
    let channels = track.codec_params.channels.expect("no channels");
    let n_frames = track.codec_params.n_frames.expect("no n_frames");

    let mut samplebuf: SampleBuffer<T> = audio::SampleBuffer::new(n_frames,
         SignalSpec::new(samplerate, channels));
    
    let mut decoder = codec.make(&track.codec_params, &DecoderOptions::default()).unwrap();
    //let mut c = 0;
    let mut vsamples: Vec<T> = Vec::<T>::new();

    loop {
        let ref packet = match format.next_packet() { Ok(p) => p, Err(_) => break, };
        let decoded = decoder.decode(packet).unwrap();
        samplebuf.copy_interleaved_ref(decoded); // if 2 channels. the first half is left, the second half is right
        vsamples.append(&mut Vec::<T>::from(samplebuf.samples()));
        //println!("{}", c);c += 1;
    }
    for i in 0..channels.count() {
        pcm_vec.push(vsamples[i..].iter().step_by(channels.count()).cloned().collect());
    }
    return Ok(samplerate);
}

fn get_variable_length_number(bytes: &mut Vec<u8>) -> u64 {
    let mut n = 0u64;

    loop {
        n <<= 7;
        let x = bytes.remove(0);
        n |= (x & 0x7F) as u64;
        if x & 0x80 == 0 {
            break;
        }
    }
    n
}

fn to_variable_length_bytes(number: usize) -> Vec<u8> {
    let mut output = Vec::new();
    let mut first_pass = true;
    let mut working_number = number;
    let mut tmp;
    while working_number > 0 || first_pass {
        tmp = working_number & 0x7F;
        working_number >>= 7;

        if ! first_pass {
            tmp |= 0x80;
        }

        output.push(tmp as u8);
        first_pass = false;
    }
    output.reverse();

    output
}

struct MidiWriterRaw {
    ppqn: u16,
    tracks: Vec<Vec<u8>>,
}

impl MidiWriterRaw {
    fn new() -> MidiWriterRaw {
        MidiWriterRaw {
            ppqn: 480,
            tracks: Vec::new(),
        }
    }
    
    fn set_ppqn(&mut self, ppqn: u16) {
        self.ppqn = ppqn;
    }

    fn add_track(&mut self) -> usize {
        self.tracks.push(Vec::new());
        self.tracks.len() - 1
    }

    fn push_event(&mut self, track: usize, wait: usize, event: MIDIEvent) {
        if track+1 >= self.tracks.len() {
            //add new tracks
            for _ in self.tracks.len()..track+1 {
                self.add_track();
            }
        }
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&to_variable_length_bytes(wait));
        bytes.extend_from_slice(&event.as_bytes());
        self.tracks[track].extend_from_slice(&bytes);
    }

    fn save(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let mut file = File::create(path)?;
        let mut header = Vec::new();
        header.extend_from_slice(b"MThd");
        header.extend_from_slice(&[0, 0, 0, 6]);
        header.extend_from_slice(&[0, 1]);
        header.extend_from_slice(&[0, self.tracks.len() as u8]);
        header.extend_from_slice(&[(self.ppqn >> 8) as u8, (self.ppqn & 0xFF) as u8]);
        file.write_all(&header)?;

        for track in &self.tracks {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(b"MTrk");
            bytes.extend_from_slice(&[(track.len() >> 24) as u8, ((track.len() >> 16) & 0xFF) as u8, ((track.len() >> 8) & 0xFF) as u8, (track.len() & 0xFF) as u8]);
            bytes.extend_from_slice(&track);
            file.write_all(&bytes)?;
        }

        Ok(())
    }
}


fn gen_midi_from_pcm(src: &Vec<Vec<i16>>, smf: &mut MidiWriterRaw, fs: u32) -> Result<u64, Box<dyn std::error::Error>> {
    
    smf.set_ppqn((fs/100).try_into().unwrap());
    smf.push_event(0, 0, apres::MIDIEvent::SetTempo(60000000/6000));

    let is_stereo: bool;
    match src.len() {
        1 => is_stereo = false,
        2 => is_stereo = true,
        _ => return Err("not mono or stereo".into()),
    }
    // program change
    smf.push_event(0, 0, apres::MIDIEvent::ProgramChange(0, 0));
    smf.push_event(1, 0, apres::MIDIEvent::ProgramChange(1, 74));
    if is_stereo {
        smf.push_event(2, 0, apres::MIDIEvent::ProgramChange(2, 0));
        smf.push_event(3, 0, apres::MIDIEvent::ProgramChange(3, 74));
    }
    // control change: panpot
    smf.push_event(0, 0, apres::MIDIEvent::ControlChange(0, 10, 1));
    smf.push_event(1, 0, apres::MIDIEvent::ControlChange(1, 10, 1));
    smf.push_event(2, 0, apres::MIDIEvent::ControlChange(2, 10, 127));
    smf.push_event(3, 0, apres::MIDIEvent::ControlChange(3, 10, 127));

    fn amp2vel(point: i16, is_right_channel: bool) -> (u8, u8) {
        const K: f32 = 127.0*127.0/32768.0;
        let result = f32::sqrt(f32::abs(point.into())*K);
        let mut u: u8 = if point >= 0 { 0 } else { 1 };
        if is_right_channel {
            u += 2;
        }
        return ((result + 0.5).floor() as u8, u.into());
    }

    let mut note_count: u64 = 0;
    print!(".________________________________________.\n|");
    std::io::stdout().flush().unwrap();
    for ch_i in 0..src.len(){
        let mut deltatimes: [usize; 4] = [0, 0, 0, 0];
        for i in 0..src[0].len() {
            let (vel, u): (u8, u8) = amp2vel(src[ch_i][i], ch_i == 1);
            if vel != 0 {
                let (d1, d2) = (deltatimes[usize::from(u)], 1); 
                note_count += 1;
                smf.push_event(usize::from(u), d1, apres::MIDIEvent::NoteOn(u, 60, vel));
                smf.push_event(usize::from(u), d2, apres::MIDIEvent::NoteOff(u, 60, vel));
            }
            for d in 0..4 {
                deltatimes[d] += 1;
            }
            if vel != 0 {
                deltatimes[usize::from(u)] = 0;
            }
            if i % (src[0].len() / 20) == 0 && i != 0 {
                print!("=");
                std::io::stdout().flush().unwrap();
            }
        }
    }
    for d in 0..4 {
        smf.push_event(d.try_into().unwrap(), 0, MIDIEvent::EndOfTrack);
    }
    println!("|");

    return Ok(note_count);
}


// parse command line arguments, first argument is the path to the file to be processed
fn main() {
    let path = env::args().nth(1).unwrap();
    let the_file: Box<dyn MediaSource> = Box::new(File::open(path.as_str()).unwrap());
    let mut arr_samples = Vec::<Vec<i16>>::new(); 
    let fs = load_media_to_pcm(the_file, &mut arr_samples).unwrap();
    //println!("{:?}",&arr_samples[0][0..1000]);
    //println!("{:?}",&arr_samples[1][0..1000]);

    let mut smf = MidiWriterRaw::new();
    let note_count = gen_midi_from_pcm(&arr_samples, &mut smf, fs).unwrap();
    smf.save(Path::new(format!("{}.PCM.mid", path.as_str()).as_str())).unwrap();
    println!("Note count: {}", note_count);
    


}

