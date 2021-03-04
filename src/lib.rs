use std::error::Error;
use std::process;
use std::io::{Read, Seek, SeekFrom, BufReader, BufRead};
use std::fs::File;

//Define some constants here
// MAX_PKZ_FILES is used to find the best_files
// candidates
const _MAX_PKZ_FILES : u8 = 8;
const FLAG_LOCAL_SIZE_UNKNOWN : u16 = 8;

#[derive(Debug)]
pub struct Zipctx{
    zipname: String,
    lfh : Vec<LFH>,
    cd : Vec<CD>,
    eocd : EOCD,
    check_bytes: u8,
    cs: String,
}

#[derive(Debug, Default)]
pub struct CD{
    ver_by : u16,
    ver_needed : u16,
    gp_flag : u16,
    comp_method : u16,

    file_crc32 : u32,
    comp_size : u32,
    uncomp_size :  u32,

    fname_len :u16,
    ef_len : u16,
    comment_len : u16,

    rel_lfh_offset : u32,
    fname_offset : usize,

    ef: Vec<u8>,
}

#[derive(Debug)]
pub struct EOCD{
    n_disk        : u16,
    cd_start_disk : u16,
    cdr_disk      : u16,
    sigma_cdr     : u16,
    cd_size       : u32,
    cd_offset     : u32,
    comment_len   : u16,
    comment       : u8,
}

#[derive(Debug)]
pub struct LFH{
    pub ver_needed : u16,
    pub gp_flag : u16,
    pub comp_method : u16,

    pub file_mod_time: u16,
    pub file_mod_date: u16,

    pub file_crc32 : u32,
    pub comp_size : u32,
    pub uncomp_size :  u32,

    pub fname_len : usize,
    pub ef_len : usize,
    pub file_name : String,

    pub magic_type : u16,
    pub offex : u16,

    pub ef:  Vec<u8>,
    hash: Vec<u8>,
}

fn u16_to_le(byteslice: &[u8]) -> u16{
    let mut v : u16 = 0;
    v = v | ((byteslice[1] as u16) << 8);
    v = v | byteslice[0] as u16;
    v
}

fn u32_to_le(byteslice: &[u8]) -> u32{
    let mut v : u32 = 0;
    v = v | ((byteslice[3] as u32) << 24);
    v = v | ((byteslice[2] as u32) << 16);
    v = v | ((byteslice[1] as u32) << 8);
    v = v | ((byteslice[0] as u32));

    v
}

impl Zipctx{
    pub fn new(zipname: String, lfheaders : Vec<LFH>, cd: Vec<CD>, eocd : EOCD ) -> Zipctx{
        Zipctx{
            zipname,
            lfh : lfheaders ,
            cd,
            eocd,
            check_bytes: 2,
            cs: "".into(),
        }
    }
}

impl CD{
    pub fn new<R: Read+Seek>(cd_offset: u32, reader : &mut R ) -> Result<Vec<CD>, Box<dyn Error>>{
        reader.seek(SeekFrom::Start(cd_offset as u64))?;
        let mut field_vec : Vec<CD> = vec![CD::default()];

        let mut cd_bytes : Vec<u8> = vec![0u8; 46];
        reader.read_exact(&mut cd_bytes).unwrap();

        let mut n_rec = 1;
        while n_rec > 0{
            let ver_by = u16_to_le(&cd_bytes[4..6]);
            let ver_needed = u16_to_le(&cd_bytes[6..8]) & 0xff;
            let gp_flag = u16_to_le(&cd_bytes[8..10]);
            let comp_method = u16_to_le(&cd_bytes[10..12]);

            // We don't really need the modification time and dates

            let file_crc32 = u32_to_le(&cd_bytes[16..20]);
            let comp_size = u32_to_le(&cd_bytes[20..24]);
            let uncomp_size = u32_to_le(&cd_bytes[24..28]);

            //Get dynamic length stuff
            let fname_len = u16_to_le(&cd_bytes[28..30]);
            let ef_len = u16_to_le(&cd_bytes[30..32]);
            let comment_len = u16_to_le(&cd_bytes[32..34]);

            let mut filename = vec![0u8;fname_len as usize];
            reader.read_exact(&mut filename).unwrap();
            filename.resize(12, 0);

            let mut ef = vec![0u8; ef_len as usize];
            reader.read_exact(&mut ef).unwrap();

            let rel_lfh_offset = u32_to_le(&cd_bytes[42..46]);
            let fname_offset = 46 + (fname_len as usize) + 1;

            let cd = CD{
                ver_by,
                ver_needed,
                gp_flag,
                comp_method,
                file_crc32,
                comp_size,
                uncomp_size,

                fname_len,
                ef_len,

                comment_len,
                rel_lfh_offset,
                fname_offset,
                ef,
            };

            field_vec.push(cd);
            n_rec -= 1;
        }

        Ok(field_vec)
    }
}

impl EOCD{
    fn new<R: Read + Seek>(reader : &mut R) -> Result<EOCD, &'static str>{
        let mut eocd_bytes : Vec<u8>= vec![0u8; 22];
        reader.seek(SeekFrom::End(-22)).unwrap();
        reader.read_exact(&mut eocd_bytes).unwrap();

        if !(eocd_bytes[0] == 0x50 && eocd_bytes[1] == 0x4b){
            return Err("Header mismatch. Exiting")
        }

        let eocd_struct = EOCD{
            n_disk: u16_to_le(&eocd_bytes[4..6]),
            cd_start_disk: u16_to_le(&eocd_bytes[6..8]),
            cdr_disk: u16_to_le(&eocd_bytes[8..10]),
            sigma_cdr: u16_to_le(&eocd_bytes[10..12]),
            cd_size: u32_to_le(&eocd_bytes[12..16]),
            cd_offset: u32_to_le(&eocd_bytes[16..20]),
            comment_len: u16_to_le(&eocd_bytes[20..22]),
            comment: 0,
        };

        Ok(eocd_struct)
    }
}

impl LFH{
    pub fn new<R: Seek+BufRead>(cd: &CD, reader: &mut R) -> Result<Vec<LFH>, Box<dyn Error>>{
        let mut lfh_bytes : Vec<u8> = vec![0u8; 30];
        reader.seek(SeekFrom::Start(cd.rel_lfh_offset as u64))?;
        reader.read_exact(&mut lfh_bytes)?;

        //The LFH is expected to be at the most 30 + n + m bytes
        // where n is fname_len and m is ef_len. So we read those
        // two first and then extend the vector accordingly
        // for reading EF and filename
        //
        let n = u16_to_le(&lfh_bytes[26..28]) as usize;
        let m = u16_to_le(&lfh_bytes[28..30]) as usize;

        //Extra fields
        let mut extra_fields : Vec<u8> = vec![0u8; (n+ m) as usize];
        reader.read_exact(&mut extra_fields)?;

        //Read the actual hash contents
        let mut hash : Vec<u8> = vec![0u8; cd.comp_size as usize];
        reader.read_exact(&mut hash)?;

        let lfh = LFH{
            ver_needed: u16_to_le(&lfh_bytes[4..6]),
            gp_flag: u16_to_le(&lfh_bytes[6..8]),
            comp_method: u16_to_le(&lfh_bytes[8..10]),
            file_mod_time: u16_to_le(&lfh_bytes[10..12]),
            file_mod_date: u16_to_le(&lfh_bytes[12..14]),

            file_crc32: u32_to_le(&lfh_bytes[14..18]),
            comp_size: u32_to_le(&lfh_bytes[18..22]),
            uncomp_size: u32_to_le(&lfh_bytes[22..26]),

            fname_len: n,
            ef_len: m,

            file_name: String::from_utf8(extra_fields[0..n].to_vec()).unwrap(),

            magic_type: 0,
            offex: 30 + (m +n) as u16,

            ef : extra_fields[n..].to_vec(),
            hash,
        };

        Ok(vec![lfh])
    }
}


fn process_legacy(zipfile: &mut Zipctx) -> Option<u8>{
    let lfh = &zipfile.lfh[0];

    // The gp_flag is set to 9 in our case , so 9 & 1 == 1 (Encrypted)
    // Else we return early from the function as it may not be encrypted
    // (My LSP names early return as guarded return)
    if !((lfh.gp_flag & 0x01) == 1 &&
         (lfh.ver_needed == 10 || lfh.ver_needed == 20 || lfh.ver_needed == 45)) {
        return None;
    }

    zipfile.cs = if (lfh.gp_flag & FLAG_LOCAL_SIZE_UNKNOWN) == 1{
        format!("{:2x}{:2x}", lfh.file_mod_time >> 8, lfh.file_mod_time & 0xff )
    } else{
        format!("{:2x}{:2x}", ((lfh.file_crc32 >> 24)) & 0xff, ((lfh.file_crc32 >>16) & 0xff))
    };

    let mut err_string = format!("ver {}.{} ", lfh.ver_needed / 10, lfh.ver_needed % 10);

    //Handle the extra fields here
    {
        let mut ef_len_used : u16 = 0;
        let ef = &lfh.ef;

        let mut efh_off : usize= 0;
        while (ef_len_used as usize) <  lfh.ef_len{
            let efh_id = u16_to_le(&ef[efh_off..efh_off+ 2]);
            let efh_ds = u16_to_le(&ef[efh_off+2..efh_off+4]); //Datasize

            err_string += format!("efh {:x} ", efh_id).as_str();

            efh_off += efh_ds as usize + 4;
            ef_len_used += 4 + efh_ds;
        }
    }

    err_string += format!("{}/{} PKZIP{} Encr: {}{} cmplen={}, decmplen={}, crc={:8x}, ts={:4x}, cs={}, type={}",
                zipfile.zipname,
                lfh.file_name,
                "", //No ZIP64, avoid
                if zipfile.check_bytes == 2 {"2b chk, "} else {""}, //This is a HACK, fix this later maybe?
                if lfh.gp_flag & FLAG_LOCAL_SIZE_UNKNOWN  == 0 {""} else {"TS_chk"},
                lfh.comp_size,
                lfh.uncomp_size,
                lfh.file_crc32,
                lfh.file_mod_time,
                zipfile.cs,
                lfh.comp_method
    ).as_str();

    Some(1)
}

fn handle_file_entry(zipfile: &mut Zipctx) -> u8{
    if process_legacy(zipfile).unwrap() == 1{
        return 1;
    };
    0
}

pub fn construct_zip(zipfile: &String) -> Result<Zipctx, Box<dyn Error>>{
    let mut reader = BufReader::new(File::open(zipfile).unwrap());

    let eocd: EOCD= EOCD::new(&mut reader).unwrap_or_else(|err| {
        eprintln!("Problem parsing EOCD : {}", err);
        process::exit(1);
    });

    let cd : Vec<CD> = CD::new(eocd.cd_offset, &mut reader).unwrap_or_else(|err|{
        eprintln!("Problem parsing EOCD : {}", err);
        process::exit(1);
    });


    let curzip_cd = &cd[1];
    let lfheaders : Vec<LFH> = LFH::new(&curzip_cd, &mut reader).unwrap_or_else(|err|{
        eprintln!("Problem parsing EOCD : {}", err);
        process::exit(1);
    });


    let mut results: Zipctx = Zipctx::new(zipfile.into(), lfheaders, cd, eocd);
   // println!("{:x?}", results);

    handle_file_entry(&mut results);
    write_and_clean(&mut results);
    Ok(results)
}

pub fn write_and_clean(ctx : &mut Zipctx){
    let lfh = ctx.lfh.pop().unwrap();

    //num_candidates = 1 for now
    print!("{}/{}:$pkzip2${:x}*", ctx.zipname, lfh.file_name, 1);

    //set offset=0, because only one file for now
    print!("{:x}*2*{:x}*{:x}*{:x}*{:x}*{:x}*{:x}*{:x}*{:x}*{}*{:x}", ctx.check_bytes, lfh.magic_type,
           lfh.comp_size, lfh.uncomp_size,
           lfh.file_crc32, 0,
           lfh.offex, lfh.comp_method,
           lfh.comp_size, ctx.cs, lfh.file_mod_time
    );

    for i in lfh.hash{
        print!("{:x}", i);
    }

    print!("$/pkzip$:{}:{}::{}\n", lfh.file_name, ctx.zipname, ctx.zipname);
}
