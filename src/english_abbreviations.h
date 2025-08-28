// EnglishAbbreviations.h
#pragma once

#include <string>
#include <unordered_set>
#include <sstream>

namespace nvs {

/// Compiled-in set of English abbreviations (e.g. Mr, Mrs, Dr).
/// Built once (thread-safe in C++11+) from the raw string literal below.
class EnglishAbbreviations {
public:
    /// Returns the singleton abbreviation set.
    static const std::unordered_set<std::string>& instance() {
        static const std::unordered_set<std::string> dict = []{
            static constexpr const char* blob = R"ABBR(
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
)ABBR";

            std::unordered_set<std::string> tmp;
            std::istringstream in{blob};
            for (std::string w; std::getline(in, w); ) {
                if (!w.empty()) tmp.insert(w);
            }
            return tmp;
        }();
        return dict;
    }

    /// Check if `abbr` is in the list.
    static bool contains(const std::string& abbr) {
        return instance().count(abbr) > 0;
    }

    /// Number of abbreviations loaded.
    static std::size_t size() {
        return instance().size();
    }

private:
    EnglishAbbreviations() = delete;
    ~EnglishAbbreviations() = delete;
    EnglishAbbreviations(const EnglishAbbreviations&) = delete;
    EnglishAbbreviations& operator=(const EnglishAbbreviations&) = delete;
};

} // namespace nvs

