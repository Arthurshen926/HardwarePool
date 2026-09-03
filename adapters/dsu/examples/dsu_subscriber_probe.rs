//! Small DSU/Cemuhook subscriber used for local-lab interoperability evidence.

use std::env;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use capyio_dsu_adapter::{DSU_PROTOCOL_VERSION, crc32_ieee};

const MESSAGE_PAD_DATA: u32 = 0x10_0002;
const PAD_PACKET_BYTES: usize = 100;

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("u32 slice"))
}

fn read_f32(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("f32 slice"))
}

fn pad_request(client_id: u32) -> [u8; 28] {
    let mut packet = [0_u8; 28];
    packet[..4].copy_from_slice(b"DSUC");
    write_u16(&mut packet, 4, DSU_PROTOCOL_VERSION);
    write_u16(&mut packet, 6, 12);
    write_u32(&mut packet, 12, client_id);
    write_u32(&mut packet, 16, MESSAGE_PAD_DATA);
    packet[20] = 1;
    packet[21] = 0;
    let checksum = crc32_ieee(&packet);
    write_u32(&mut packet, 8, checksum);
    packet
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let target: SocketAddr = args
        .next()
        .unwrap_or_else(|| "127.0.0.1:26760".to_owned())
        .parse()?;
    let wanted_packets: u32 = args.next().unwrap_or_else(|| "180".to_owned()).parse()?;
    if wanted_packets == 0 {
        return Err("packet count must be positive".into());
    }

    let socket = UdpSocket::bind("127.0.0.1:0")?;
    socket.set_read_timeout(Some(Duration::from_secs(2)))?;
    socket.send_to(&pad_request(0x4341_5059), target)?;

    let started = Instant::now();
    let mut buffer = [0_u8; PAD_PACKET_BYTES];
    let mut received = 0_u32;
    let mut first_packet_number = None;
    let mut last_packet_number = 0_u32;
    let mut digital_or = [0_u8; 3];
    let mut left_stick_x = (u8::MAX, u8::MIN);
    let mut left_stick_y = (u8::MAX, u8::MIN);
    let mut right_stick_x = (u8::MAX, u8::MIN);
    let mut right_stick_y = (u8::MAX, u8::MIN);
    let mut max_left_trigger = 0_u8;
    let mut max_right_trigger = 0_u8;
    let mut accel_min = [f32::INFINITY; 3];
    let mut accel_max = [f32::NEG_INFINITY; 3];
    let mut gyro_min = [f32::INFINITY; 3];
    let mut gyro_max = [f32::NEG_INFINITY; 3];

    while received < wanted_packets {
        let (bytes, source) = socket.recv_from(&mut buffer)?;
        if bytes != PAD_PACKET_BYTES
            || source != target
            || &buffer[..4] != b"DSUS"
            || read_u32(&buffer, 16) != MESSAGE_PAD_DATA
        {
            continue;
        }

        let packet_number = read_u32(&buffer, 32);
        first_packet_number.get_or_insert(packet_number);
        last_packet_number = packet_number;
        digital_or[0] |= buffer[36];
        digital_or[1] |= buffer[37];
        digital_or[2] |= buffer[38];
        left_stick_x.0 = left_stick_x.0.min(buffer[40]);
        left_stick_x.1 = left_stick_x.1.max(buffer[40]);
        left_stick_y.0 = left_stick_y.0.min(buffer[41]);
        left_stick_y.1 = left_stick_y.1.max(buffer[41]);
        right_stick_x.0 = right_stick_x.0.min(buffer[42]);
        right_stick_x.1 = right_stick_x.1.max(buffer[42]);
        right_stick_y.0 = right_stick_y.0.min(buffer[43]);
        right_stick_y.1 = right_stick_y.1.max(buffer[43]);
        max_right_trigger = max_right_trigger.max(buffer[54]);
        max_left_trigger = max_left_trigger.max(buffer[55]);

        for (index, offset) in [76, 80, 84].into_iter().enumerate() {
            let value = read_f32(&buffer, offset);
            accel_min[index] = accel_min[index].min(value);
            accel_max[index] = accel_max[index].max(value);
        }
        for (index, offset) in [88, 92, 96].into_iter().enumerate() {
            let value = read_f32(&buffer, offset);
            gyro_min[index] = gyro_min[index].min(value);
            gyro_max[index] = gyro_max[index].max(value);
        }
        received += 1;
    }

    println!("target={target}");
    println!("packets={received}");
    println!(
        "packet_numbers={}..={last_packet_number}",
        first_packet_number.expect("at least one packet")
    );
    println!("elapsed_ms={}", started.elapsed().as_millis());
    println!(
        "digital_or={:02x},{:02x},{:02x}",
        digital_or[0], digital_or[1], digital_or[2]
    );
    println!(
        "sticks_lx={}..{} ly={}..{} rx={}..{} ry={}..{}",
        left_stick_x.0,
        left_stick_x.1,
        left_stick_y.0,
        left_stick_y.1,
        right_stick_x.0,
        right_stick_x.1,
        right_stick_y.0,
        right_stick_y.1
    );
    println!("triggers_l={max_left_trigger} r={max_right_trigger}");
    println!("accel_g_min={accel_min:?} max={accel_max:?}");
    println!("gyro_dps_min={gyro_min:?} max={gyro_max:?}");
    Ok(())
}
