use std::collections::HashSet;
use std::sync::OnceLock;

static ABBR: OnceLock<HashSet<String>> = OnceLock::new();

fn build() -> HashSet<String> {
    const BLOB: &str = r#"
acc
ad
anon
arr
assoc
atty
av
ave
b
bart
brig
bros
bur
cap
capt
cf
chap
cm
co
col
comb
compar
cont
contd
contr
corp
cu
d
deg
dept
dia
dist
div
doc
doz
dr
ed
eds
eg
eqn
eqns
esp
est
et al
etc
ex
f
fem
ff
fig
figs
for
ft
gm
gov
hms
hon
hr
ib
ibid
in
inc
ins
inst
jr
jnr
kg
km
lbs
ld
ltd
maj
masc
met
miss
min
mil
mm
mme
mr
mrs
ms
mssr
mssrs
mt
mp
neg
no
nol
nom
nos
oz
ox
pass
pers
phr
pl
poss
pres
prof
prop
plc
ref
refl
rep
repr
rev
revd
rt
sec
seq
sen
sing
sr
ss
subsp
superl
supt
stat
t
tech
trans
usu
v
var
viz
vol
vols
vp
vs
yr
yrs
jan
feb
mar
apr
jun
jul
aug
sep
sept
oct
nov
dec
mon
tues
wed
thur
thu
fri
sat
sun
"#;
    let mut set = HashSet::new();
    for line in BLOB.lines() {
        let w = line.trim();
        if !w.is_empty() {
            set.insert(w.to_string());
        }
    }
    set
}

pub fn contains(abbr: &str) -> bool {
    // Case-insensitive: sets are lowercase
    ABBR.get_or_init(build).contains(&abbr.to_ascii_lowercase())
}

pub fn size() -> usize {
    ABBR.get_or_init(build).len()
}
