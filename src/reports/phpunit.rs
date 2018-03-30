use std::io::Read;

use xmltree::Element;
use failure::Error;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Runtime {
    pub name: String,
    pub version: String,
    pub url: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Driver {
    pub name: String,
    pub version: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Build {
    pub time: String,
    pub phpunit: String,
    pub coverage: String,

    pub runtime: Runtime,
    pub driver: Driver,
}

impl Build {
    fn from_elem(el: &Element) -> Result<Self, Error> {

        // Handle the <runtime> tag.

        let relem = elem
            .get_child("runtime")
            .ok_or(format_err!("build.runtime tag not found"))?;

        let name = relem
            .attributes
            .get("name")
            .ok_or(format_err!("build.runtime.name attribute not found"))?;

        let version = relem
            .attributes
            .get("version")
            .ok_or(format_err!("build.runtime.version attribute not found"))?;

        let url = relem
            .attributes
            .get("url")
            .ok_or(format_err!("build.runtime.url attribute not found"))?;

        let runtime = Runtime{
            name,
            version,
            url,
        };

        let delem = elem
            .get_child("driver")
            .ok_or(format_err!("build.driver tag not found"))?;

        let name = delem
            .attributes
            .get("name")
            .ok_or(format_err!("build.runtime.name attribute not found"))?;

        let version = delem
            .attributes
            .get("version")
            .ok_or(format_err!("build.runtime.version attribute not found"))?;

        let driver = Driver{
            name,
            version,
        };

        let time = elem
            .attributes
            .get("time")
            .ok_or(format_err!("build.time attribute not found"))?;

        let phpunit = elem
            .attributes
            .get("phpunit")
            .ok_or(format_err!("build.phpunit attribute not found"))?;

        let coverage = elem
            .attributes
            .get("coverage")
            .ok_or(format_err!("build.coverage attribute not found"))?;

        Ok(Build{
            time,
            phpunit,
            coverage,
            runtime,
            driver,
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Test {
    pub name: String,
    pub size: String,
    pub result: String,
    pub status: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LineTotals {
    total: u64,
    comments: u64,
    code: u64,
    executable: u64,
    executed: u64,
    percent: f64
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ItemTotals {
    pub count: u64,
    pub tested: u64,
    pub percent: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Totals {
    pub lines: LineTotals,
    pub methods: ItemTotals,
    pub functions: ItemTotals,
    pub classes: ItemTotals,
    pub traits: ItemTotals,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct File {
    pub name: String,
    pub href: String,
    pub totals: Totals,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Directory {
    pub totals: Totals,
    pub directories: Vec<Directory>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Project {
    pub source: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PhpunitReport {
    pub build: Build,
    pub project: Project,
    pub tests: Vec<Test>,
    pub directory: Directory,
}

impl PhpunitReport {
    pub fn parse<I: Read>(mut input: I) -> Result<Self, ::failure::Error> {
        let tree = ::xmltree::Element::parse(input)?;

        let elem = tree
            .get_child("build")
            .ok_or(format_err!("<build> tag not found"))?;



        let coverage = elem
            .attributes
            .get("coverage");
            .ok_or(format_err!("build.coverage attribute not found"))?;



        let r = PhpunitReport{
        };

        Ok(r)
    }
}