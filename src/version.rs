use std::error::Error;
use std::cmp::Ordering;

#[derive(Copy, Clone, Eq)]
pub struct Version {
    versionBytes:[u8;4],
    versionHash:u32,
}

impl Version {
    pub fn parse( string:&String ) -> Result< Version, String >{
        let mut v=[0u32;4];
        let mut c=0;

        for ns in string.split('.'){
            match ns.parse::<u32>(){
                Ok( n ) => {
                    if n>255 {
                        return Err( format!("Max value of part of version must be less then 256"));
                    }

                    if c>=4 {
                        return Err( format!("Version is too long, version should have 4 parts like *.*.*.*"));
                    }

                    v[c]=n;

                    c+=1;
                },
                Err( e )=>return Err( format!("Can not parse version: {}", e.description())),
            }
        }

        if c!=4 {
            return Err( format!("Version is too short, version should have 4 parts like *.*.*.*"));
        }

        let versionHash=v[0] * 16777216+
                        v[1] * 65536+
                        v[2] * 256+
                        v[3];

        Ok(Version{
            versionBytes:[v[0] as u8, v[1] as u8, v[2] as u8, v[3] as u8],
            versionHash:versionHash,
        })
    }

    pub fn print(&self) -> String{
        let v=&self.versionBytes;
        format!("{}.{}.{}.{}",v[0],v[1],v[2],v[3])
    }
}

impl PartialOrd for Version{
    fn partial_cmp(&self, other: &Version) -> Option<Ordering> {
        Some(self.cmp(other))
    }
    /*
    fn lt(&self, other: &Self) -> bool { self.versionHash<other.versionHash }
    fn le(&self, other: &Self) -> bool { self.versionHash<=other.versionHash }
    fn gt(&self, other: &Self) -> bool { self.versionHash>other.versionHash }
    fn ge(&self, other: &Self) -> bool { self.versionHash>=other.versionHash }
    */
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.versionHash.cmp(&other.versionHash)
    }
}

impl PartialEq for Version{
    fn eq(&self, other: &Self) -> bool { self.versionHash==other.versionHash }
    fn ne(&self, other: &Self) -> bool { self.versionHash!=other.versionHash }
}
