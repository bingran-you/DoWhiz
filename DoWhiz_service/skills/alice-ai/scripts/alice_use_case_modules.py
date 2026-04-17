#!/usr/bin/env python3

"""Step 9 use-case screening modules and directional economics for Alice AI."""

from __future__ import annotations

import re
from typing import Any


ALL_MODULE_IDS = [
    "energy_solar",
    "energy_wind",
    "energy_battery",
    "agriculture_general",
    "residential_light_development",
    "industrial_storage",
    "recreational_rural_hold",
]

MODULE_DEFINITIONS = {
    "energy_solar": {
        "family": "energy",
        "label": "Solar Energy Screening",
    },
    "energy_wind": {
        "family": "energy",
        "label": "Wind Energy Screening",
    },
    "energy_battery": {
        "family": "energy",
        "label": "Battery Energy Storage Screening",
    },
    "agriculture_general": {
        "family": "agriculture",
        "label": "Agriculture Screening",
    },
    "residential_light_development": {
        "family": "residential_light_development",
        "label": "Residential / Light Development Screening",
    },
    "industrial_storage": {
        "family": "industrial_storage_commercial",
        "label": "Industrial / Storage Screening",
    },
    "recreational_rural_hold": {
        "family": "recreational_rural_hold",
        "label": "Recreational / Rural Hold Screening",
    },
}

FAMILY_TO_MODULE_IDS = {
    "energy": ["energy_solar", "energy_wind", "energy_battery"],
    "agriculture": ["agriculture_general"],
    "residential_light_development": ["residential_light_development"],
    "industrial_storage_commercial": ["industrial_storage"],
    "recreational_rural_hold": ["recreational_rural_hold"],
}

MODULE_ORDER = {
    module_id: index
    for index, module_id in enumerate(ALL_MODULE_IDS)
}

FIT_ORDER = {
    "favorable": 0,
    "mixed": 1,
    "weak": 2,
    "unknown": 3,
    "not_applicable": 4,
}

MODULE_STATUS_ORDER = {
    "evaluated": 0,
    "partially_evaluated": 1,
    "insufficient_data": 2,
    "blocked": 3,
}

SCOPE_PRIORITY = {
    "parcel_confirmed": 0,
    "parcel_candidate": 1,
    "county_level": 2,
    "geography_only": 3,
    "listing_derived": 4,
    "inferred": 5,
    "unresolved": 6,
}

RISKY_LOCAL_FOOTING = {
    "federal_baseline_only",
    "minimal_local_footing",
}

SOLAR_HINTS = ("solar", "photovoltaic", "pv")
WIND_HINTS = ("wind", "turbine")
BATTERY_HINTS = ("battery", "bess", "storage")
AG_HINTS = ("agriculture", "grazing", "pasture", "crop", "farm", "ranch", "irrig")
RESIDENTIAL_HINTS = ("residential", "homesite", "home site", "subdivide", "subdivision", "build")
INDUSTRIAL_HINTS = ("industrial", "commercial", "yard", "warehouse", "outdoor storage")
RURAL_HOLD_HINTS = ("rural hold", "hold", "recreation", "recreational", "hunting", "retreat")


def bounded(value: float) -> float:
    return max(0.0, min(1.0, round(value, 3)))


def sorted_unique_strings(values: list[str]) -> list[str]:
    return sorted({value for value in values if value})


def evidence_field(
    value: Any = None,
    *,
    status: str = "missing",
    confidence: float = 0.0,
    evidence_scope: str = "unresolved",
    supporting_item_ids: list[str] | None = None,
    linked_candidate_ids: list[str] | None = None,
    citation_ids: list[str] | None = None,
    as_of: str | None = None,
    notes: str | None = None,
) -> dict[str, Any]:
    return {
        "value": value,
        "status": status,
        "confidence": bounded(confidence),
        "evidence_scope": evidence_scope,
        "supporting_item_ids": sorted_unique_strings(supporting_item_ids or []),
        "linked_candidate_ids": sorted_unique_strings(linked_candidate_ids or []),
        "citation_ids": sorted_unique_strings(citation_ids or []),
        "as_of": as_of,
        "notes": notes,
    }


def _field_value(field: dict[str, Any] | None) -> Any:
    if not isinstance(field, dict):
        return None
    return field.get("value")


def _field_status(field: dict[str, Any] | None) -> str:
    if not isinstance(field, dict):
        return "missing"
    return field.get("status") or "missing"


def _field_scope(field: dict[str, Any] | None) -> str:
    if not isinstance(field, dict):
        return "unresolved"
    return field.get("evidence_scope") or "unresolved"


def _field_confidence(field: dict[str, Any] | None) -> float:
    if not isinstance(field, dict):
        return 0.0
    raw = field.get("confidence")
    if not isinstance(raw, (int, float)):
        return 0.0
    return bounded(float(raw))


def _field_citation_ids(field: dict[str, Any] | None) -> list[str]:
    if not isinstance(field, dict):
        return []
    return sorted_unique_strings(list(field.get("citation_ids") or []))


def _has_value(field: dict[str, Any] | None) -> bool:
    if _field_status(field) in {"missing", "not_applicable"}:
        return False
    value = _field_value(field)
    return value is not None


def _stringify(value: Any) -> str:
    if value is None:
        return ""
    if isinstance(value, str):
        return value.strip()
    if isinstance(value, dict):
        return " ".join(_stringify(part) for part in value.values() if part)
    if isinstance(value, list):
        return " ".join(_stringify(part) for part in value)
    return str(value)


def _field_text(field: dict[str, Any] | None) -> str:
    return _stringify(_field_value(field))


def _contains_any(text: str, needles: tuple[str, ...]) -> bool:
    lowered = text.lower()
    return any(needle in lowered for needle in needles)


def _extract_first_number(text: str) -> float | None:
    match = re.search(r"(-?\d+(?:\.\d+)?)", text)
    return float(match.group(1)) if match else None


def _extract_slope_percent(text: str) -> float | None:
    match = re.search(r"slope\s+(\d+(?:\.\d+)?)\s*%", text, flags=re.IGNORECASE)
    if match:
        return float(match.group(1))
    match = re.search(
        r"(\d+(?:\.\d+)?)\s*(?:to|-)\s*(\d+(?:\.\d+)?)\s*percent slopes",
        text,
        flags=re.IGNORECASE,
    )
    if match:
        return float(match.group(2))
    match = re.search(r"(\d+(?:\.\d+)?)\s*percent slopes", text, flags=re.IGNORECASE)
    if match:
        return float(match.group(1))
    return None


def _extract_capability_class(text: str) -> int | None:
    match = re.search(r"capability class\s+(\d+)", text, flags=re.IGNORECASE)
    return int(match.group(1)) if match else None


def _primary_parcel(parcel_memo: dict[str, Any]) -> dict[str, Any] | None:
    parcel_identity = parcel_memo["parcel_identity"]
    primary_candidate_id = parcel_identity.get("primary_candidate_id")
    for parcel in parcel_identity.get("parcels", []):
        if parcel.get("candidate_id") == primary_candidate_id:
            return parcel
    parcels = parcel_identity.get("parcels", [])
    return parcels[0] if parcels else None


def _request_text(request: dict[str, Any]) -> str:
    thesis = request.get("thesis", {})
    text_parts = [
        request.get("raw_user_message"),
        thesis.get("summary"),
        " ".join(thesis.get("return_preferences", {}).get("preferred_exit_strategies", [])),
        " ".join(thesis.get("filters", {}).get("notes", [])),
        " ".join(request.get("follow_up_context", {}).get("notes", [])),
    ]
    return " ".join(part for part in text_parts if isinstance(part, str)).lower()


def _energy_module_hints(text: str) -> list[str]:
    hinted: list[str] = []
    if _contains_any(text, SOLAR_HINTS):
        hinted.append("energy_solar")
    if _contains_any(text, WIND_HINTS):
        hinted.append("energy_wind")
    if _contains_any(text, BATTERY_HINTS):
        hinted.append("energy_battery")
    return hinted


def select_module_ids(request: dict[str, Any]) -> list[str]:
    thesis = request.get("thesis", {})
    requested_families = list(thesis.get("use_case_hypotheses", []))
    request_text = _request_text(request)
    explicit_energy_modules = _energy_module_hints(request_text)
    selected: list[str] = []

    for family in requested_families:
        if family == "energy":
            selected.extend(explicit_energy_modules or FAMILY_TO_MODULE_IDS["energy"])
            continue
        selected.extend(FAMILY_TO_MODULE_IDS.get(family, []))

    if _contains_any(request_text, AG_HINTS):
        selected.append("agriculture_general")
    if _contains_any(request_text, RESIDENTIAL_HINTS):
        selected.append("residential_light_development")
    if _contains_any(request_text, INDUSTRIAL_HINTS):
        selected.append("industrial_storage")
    if _contains_any(request_text, RURAL_HOLD_HINTS):
        selected.append("recreational_rural_hold")
    if _contains_any(request_text, BATTERY_HINTS) and "energy_battery" not in selected:
        selected.append("energy_battery")
    if _contains_any(request_text, SOLAR_HINTS) and "energy_solar" not in selected:
        selected.append("energy_solar")
    if _contains_any(request_text, WIND_HINTS) and "energy_wind" not in selected:
        selected.append("energy_wind")

    if not selected:
        selected = list(ALL_MODULE_IDS)

    return sorted(
        {module_id for module_id in selected if module_id in MODULE_DEFINITIONS},
        key=lambda module_id: MODULE_ORDER[module_id],
    )


def _module_context(
    request: dict[str, Any],
    parcel_memo: dict[str, Any],
    coverage_assessment: dict[str, Any] | None,
) -> dict[str, Any]:
    listing = parcel_memo["listing"]
    market = parcel_memo["market_signals"]
    planning = parcel_memo["planning_and_land_use"]
    environmental = parcel_memo["environmental_constraints"]
    water_ag = parcel_memo["water_and_agriculture"]
    infrastructure = parcel_memo["infrastructure_and_utilities"]
    parcel_identity = parcel_memo["parcel_identity"]
    primary_parcel = _primary_parcel(parcel_memo)

    fields = {
        "listing_asking_price": listing["asking_price"],
        "listing_acreage": listing["listed_acreage"],
        "listing_text": listing["listing_text_summary"],
        "market_ask_per_acre": market["ask_price_per_acre"],
        "market_summary": market["market_summary"],
        "planning_zoning": planning["zoning_designation"],
        "planning_summary": planning["development_constraints_summary"],
        "env_flood": environmental["flood_zone"],
        "env_wetlands": environmental["wetlands_signal"],
        "env_slope": environmental["slope_signal"],
        "env_summary": environmental["environmental_summary"],
        "water_summary": water_ag["water_and_ag_summary"],
        "water_soils": water_ag["soil_productivity_signal"],
        "water_rights": water_ag["water_rights_status"],
        "infra_access": infrastructure["legal_or_physical_access_signal"],
        "infra_power": infrastructure["power_service_signal"],
        "infra_utility": infrastructure["utility_territory_signal"],
        "infra_substation": infrastructure["substation_proximity_signal"],
        "infra_transmission": infrastructure["transmission_proximity_signal"],
        "infra_broadband": infrastructure["broadband_signal"],
        "infra_water": infrastructure["water_service_signal"],
        "infra_septic": infrastructure["wastewater_or_septic_signal"],
        "infra_logistics": infrastructure["rail_or_highway_logistics_signal"],
        "infra_summary": infrastructure["infrastructure_summary"],
        "subject_address": parcel_memo["subject"]["canonical_address"],
        "subject_coordinates": parcel_memo["subject"]["coordinates"],
    }
    if primary_parcel is not None:
        fields.update(
            {
                "parcel_apn": primary_parcel["apn"],
                "parcel_address": primary_parcel["assessor_site_address"],
                "parcel_acreage": primary_parcel["assessor_acreage"],
            }
        )

    listing_text = _field_text(fields["listing_text"])
    env_summary_text = " ".join(
        filter(
            None,
            [
                _field_text(fields["env_summary"]),
                _field_text(fields["env_flood"]),
                _field_text(fields["env_wetlands"]),
                _field_text(fields["env_slope"]),
            ],
        )
    ).lower()
    water_text = " ".join(
        filter(
            None,
            [
                _field_text(fields["water_summary"]),
                _field_text(fields["water_soils"]),
            ],
        )
    ).lower()
    infra_text = " ".join(
        filter(
            None,
            [
                _field_text(fields["infra_summary"]),
                _field_text(fields["infra_access"]),
                _field_text(fields["infra_power"]),
                _field_text(fields["infra_utility"]),
                _field_text(fields["infra_substation"]),
                _field_text(fields["infra_transmission"]),
                _field_text(fields["infra_logistics"]),
            ],
        )
    ).lower()
    request_text = _request_text(request)
    combined_text = " ".join(filter(None, [request_text, listing_text.lower(), infra_text, water_text, env_summary_text]))
    acreage = _field_value(fields["listing_acreage"])
    if not isinstance(acreage, (int, float)):
        acreage = _field_value(fields.get("parcel_acreage"))

    max_purchase_price = request.get("thesis", {}).get("budget", {}).get("max_purchase_price")
    ask_price = _field_value(fields["listing_asking_price"])
    ask_per_acre = _field_value(fields["market_ask_per_acre"])
    slope_percent = _extract_slope_percent(env_summary_text)
    capability_class = _extract_capability_class(water_text)
    return {
        "fields": fields,
        "parcel_identity": parcel_identity,
        "primary_parcel": primary_parcel,
        "coverage_assessment": coverage_assessment or {},
        "request": request,
        "request_text": request_text,
        "combined_text": combined_text,
        "listing_text": listing_text.lower(),
        "env_summary_text": env_summary_text,
        "water_text": water_text,
        "infra_text": infra_text,
        "ask_price": float(ask_price) if isinstance(ask_price, (int, float)) else None,
        "ask_per_acre": float(ask_per_acre) if isinstance(ask_per_acre, (int, float)) else None,
        "acreage": float(acreage) if isinstance(acreage, (int, float)) else None,
        "max_purchase_price": float(max_purchase_price) if isinstance(max_purchase_price, (int, float)) else None,
        "hold_period": request.get("thesis", {}).get("hold_period", {}),
        "preferred_exits": list(
            request.get("thesis", {}).get("return_preferences", {}).get("preferred_exit_strategies", [])
        ),
        "filters": request.get("thesis", {}).get("filters", {}),
        "candidate_set_status": parcel_identity["candidate_set_status"],
        "overall_confirmation_level": parcel_identity["overall_confirmation_level"],
        "coverage_tier": (coverage_assessment or {}).get("effective_coverage_tier"),
        "local_footing_status": (coverage_assessment or {}).get("local_footing_status"),
        "only_federal_baseline_available": bool(
            (coverage_assessment or {}).get("source_summary", {}).get("only_federal_baseline_available")
        ),
        "missing_surfaces": set((coverage_assessment or {}).get("missing_surfaces", [])),
        "slope_percent": slope_percent,
        "capability_class": capability_class,
    }


def _add_field(field: dict[str, Any] | None, citations: set[str], scopes: set[str], confidences: list[float]) -> None:
    if not _has_value(field):
        return
    citations.update(_field_citation_ids(field))
    scopes.add(_field_scope(field))
    confidences.append(_field_confidence(field))


def _identity_support(context: dict[str, Any]) -> tuple[str | None, list[dict[str, Any]]]:
    fields: list[dict[str, Any]] = []
    primary_parcel = context["primary_parcel"]
    if primary_parcel is not None:
        fields.extend([primary_parcel.get("apn"), primary_parcel.get("assessor_site_address")])
    level = context["overall_confirmation_level"]
    if level == "parcel_confirmed":
        return "Parcel identity is already parcel-confirmed, which reduces subject mismatch risk.", fields
    if level == "candidate_corroborated":
        return "One parcel candidate is locally corroborated, but parcel-specific module conclusions remain provisional.", fields
    if level in {"candidate_unconfirmed", "listing_hint_only", "geography_only"}:
        return "The active subject still relies on a weak parcel candidate, so parcel-specific screening stays provisional.", fields
    return None, fields


def _identity_blockers(context: dict[str, Any]) -> list[str]:
    if context["candidate_set_status"] == "multiple_competing_candidates":
        return [
            "Competing parcel candidates remain active, so parcel-specific thesis conclusions cannot be treated as final."
        ]
    if context["overall_confirmation_level"] in {"candidate_unconfirmed", "listing_hint_only", "geography_only", "unresolved"}:
        return [
            "Parcel identity is still weak, so parcel-specific thesis conclusions remain vulnerable to mis-attachment."
        ]
    return []


def _module_status(
    *,
    supporting_count: int,
    blocker_count: int,
    parcel_identity_blocked: bool,
    critical_surface_missing: bool,
    only_federal: bool,
) -> str:
    if parcel_identity_blocked and supporting_count == 0:
        return "blocked"
    if supporting_count >= 3 and not only_federal:
        return "evaluated"
    if supporting_count >= 1 or blocker_count >= 1 or critical_surface_missing:
        return "partially_evaluated"
    return "insufficient_data"


def _module_fit(
    *,
    support_score: int,
    blocker_count: int,
    status: str,
    allow_favorable: bool,
) -> str:
    if status == "insufficient_data":
        return "unknown"
    if status == "blocked":
        return "weak"
    if allow_favorable and support_score >= 5 and blocker_count == 0:
        return "favorable"
    if support_score >= 2 and blocker_count <= 2:
        return "mixed"
    if support_score >= 1 or blocker_count >= 1:
        return "weak"
    return "unknown"


def _module_confidence(
    *,
    confidences: list[float],
    support_count: int,
    blocker_count: int,
    context: dict[str, Any],
    module_status: str,
    fit_assessment: str,
) -> float:
    base = 0.22
    if confidences:
        base += min(sum(confidences) / len(confidences), 0.8) * 0.35
    base += min(support_count, 4) * 0.06
    if context["overall_confirmation_level"] == "parcel_confirmed":
        base += 0.1
    elif context["overall_confirmation_level"] == "candidate_corroborated":
        base += 0.04
    if context["candidate_set_status"] == "multiple_competing_candidates":
        base -= 0.08
    if context["only_federal_baseline_available"]:
        base -= 0.08
    base -= min(blocker_count, 3) * 0.04
    if module_status == "insufficient_data":
        base = min(base, 0.38)
    if fit_assessment == "unknown":
        base = min(base, 0.42)
    return bounded(base)


def _module_scope_summary(context: dict[str, Any], scopes: set[str]) -> dict[str, Any]:
    confirmation_level = context["overall_confirmation_level"]
    dominant_scope = {
        "parcel_confirmed": "parcel_confirmed",
        "candidate_corroborated": "parcel_candidate",
        "candidate_unconfirmed": "parcel_candidate",
        "listing_hint_only": "listing_derived",
        "geography_only": "geography_only",
    }.get(confirmation_level, None)
    available_scopes = {scope for scope in scopes if scope}
    if dominant_scope is None:
        if available_scopes:
            dominant_scope = sorted(available_scopes, key=lambda scope: SCOPE_PRIORITY.get(scope, 99))[0]
        else:
            dominant_scope = "unresolved"
    available_scopes.add(dominant_scope)
    supporting_scopes = sorted(
        {scope for scope in available_scopes if scope != dominant_scope},
        key=lambda scope: SCOPE_PRIORITY.get(scope, 99),
    )
    if not available_scopes or available_scopes == {"unresolved"}:
        notes = "Module reasoning stays thin because no strong supporting evidence scope is populated yet."
    elif supporting_scopes:
        notes = (
            "Module reasoning mixes "
            f"{dominant_scope.replace('_', ' ')} footing with "
            f"{', '.join(scope.replace('_', ' ') for scope in supporting_scopes)} signals."
        )
    else:
        notes = f"Module reasoning is dominated by {dominant_scope.replace('_', ' ')} evidence."
    return {
        "dominant_scope": dominant_scope,
        "supporting_scopes": supporting_scopes,
        "notes": notes,
    }


def _compose_summary(
    *,
    label: str,
    fit_assessment: str,
    supporting_signals: list[str],
    blocking_flags: list[str],
    key_unknowns: list[str],
) -> str:
    lead = {
        "favorable": "Screening looks directionally favorable at screening grade.",
        "mixed": "Screening has usable supporting signals, but material blockers or diligence gaps remain.",
        "weak": "Screening is constrained at screening grade by current blockers and weak footing.",
        "unknown": "Screening does not yet have enough defensible evidence for a strong call.",
        "not_applicable": "Screening is not applicable to the current request.",
    }[fit_assessment]
    parts = [lead]
    if supporting_signals:
        parts.append(supporting_signals[0])
    if blocking_flags:
        parts.append(blocking_flags[0])
    if key_unknowns:
        parts.append(f"Key unknown: {key_unknowns[0]}")
    return " ".join(parts)


def _new_module(module_id: str) -> dict[str, Any]:
    definition = MODULE_DEFINITIONS[module_id]
    return {
        "module_id": module_id,
        "module_family": definition["family"],
        "module_label": definition["label"],
        "module_status": "insufficient_data",
        "fit_assessment": "unknown",
        "confidence": 0.0,
        "blocking_flags": [],
        "supporting_signals": [],
        "key_unknowns": [],
        "economics_inputs_required": [],
        "evidence_scope_summary": {
            "dominant_scope": "unresolved",
            "supporting_scopes": [],
            "notes": "Module reasoning has not been populated yet.",
        },
        "summary": "",
        "citation_ids": [],
    }


def _evaluate_energy_solar(context: dict[str, Any]) -> dict[str, Any]:
    module = _new_module("energy_solar")
    citations: set[str] = set()
    scopes: set[str] = set()
    confidences: list[float] = []
    support_score = 0
    parcel_identity_blocked = False

    identity_support, identity_fields = _identity_support(context)
    if identity_support:
        module["supporting_signals"].append(identity_support)
        support_score += 1 if context["overall_confirmation_level"] == "parcel_confirmed" else 0
        for field in identity_fields:
            _add_field(field, citations, scopes, confidences)
    identity_blockers = _identity_blockers(context)
    if identity_blockers:
        module["blocking_flags"].extend(identity_blockers)
        parcel_identity_blocked = True

    acreage = context["acreage"]
    if acreage is not None and acreage >= 20:
        module["supporting_signals"].append(
            f"Available acreage ({acreage:,.0f} acres) is large enough for an initial small-to-medium solar screen."
        )
        support_score += 2 if acreage >= 40 else 1
        _add_field(context["fields"]["listing_acreage"], citations, scopes, confidences)
        _add_field(context["fields"].get("parcel_acreage"), citations, scopes, confidences)
    elif acreage is not None:
        module["blocking_flags"].append(
            f"Only about {acreage:,.0f} acres are currently evidenced, which may be tight for a meaningful solar layout."
        )

    if context["slope_percent"] is not None and context["slope_percent"] <= 5:
        module["supporting_signals"].append(
            f"Point-based terrain clues show roughly {context['slope_percent']:.0f}% slope, which is directionally favorable for solar siting."
        )
        support_score += 1
        _add_field(context["fields"]["env_slope"], citations, scopes, confidences)

    if "no intersecting fema flood-hazard zone" in context["env_summary_text"]:
        module["supporting_signals"].append(
            "Point-based flood screening did not return a FEMA flood-hazard hit at the sampled location."
        )
        support_score += 1
        _add_field(context["fields"]["env_flood"], citations, scopes, confidences)
    if "no mapped wetland polygon" in context["env_summary_text"]:
        module["supporting_signals"].append(
            "Point-based wetlands screening did not return a mapped wetland hit at the sampled location."
        )
        support_score += 1
        _add_field(context["fields"]["env_wetlands"], citations, scopes, confidences)

    if _contains_any(context["infra_text"], ("transmission", "electricity maps", "utility", "substation", "power")):
        module["supporting_signals"].append(
            "Transmission or utility mapping entry points are available, which is helpful for a next-step power screen."
        )
        support_score += 1
        _add_field(context["fields"]["infra_summary"], citations, scopes, confidences)
        _add_field(context["fields"]["infra_transmission"], citations, scopes, confidences)
        _add_field(context["fields"]["infra_power"], citations, scopes, confidences)

    if _contains_any(context["listing_text"], ("road", "access", "frontage")) or "road" in context["infra_text"]:
        module["supporting_signals"].append(
            "Road or access language is present, although legal access still needs direct confirmation."
        )
        support_score += 1
        _add_field(context["fields"]["infra_access"], citations, scopes, confidences)
        _add_field(context["fields"]["listing_text"], citations, scopes, confidences)

    if context["max_purchase_price"] is not None and context["ask_price"] is not None:
        if context["ask_price"] <= context["max_purchase_price"]:
            module["supporting_signals"].append(
                "The current listing ask sits inside the user-stated purchase budget."
            )
            support_score += 1
        else:
            module["blocking_flags"].append(
                "The current listing ask is above the user-stated budget ceiling."
            )
        _add_field(context["fields"]["listing_asking_price"], citations, scopes, confidences)

    if "zoning" in context["missing_surfaces"]:
        module["blocking_flags"].append(
            "No authoritative zoning or local solar-compatibility check is attached yet."
        )
    elif _has_value(context["fields"]["planning_zoning"]):
        _add_field(context["fields"]["planning_zoning"], citations, scopes, confidences)

    if context["only_federal_baseline_available"]:
        module["blocking_flags"].append(
            "Local county planning and utility footing is still thin, so solar screening stays preliminary."
        )

    module["key_unknowns"] = [
        "Interconnection feasibility, queue posture, and upgrade cost are not yet known.",
        "The authoritative zoning or permitting path for solar has not been confirmed.",
        "Boundary-based environmental checks are still weaker than parcel geometry would allow.",
    ]
    module["economics_inputs_required"] = [
        "purchase basis or site-control structure",
        "interconnection or utility-upgrade cost proxy",
        "local permitting and entitlement timeline",
    ]

    module["module_status"] = _module_status(
        supporting_count=len(module["supporting_signals"]),
        blocker_count=len(module["blocking_flags"]),
        parcel_identity_blocked=parcel_identity_blocked,
        critical_surface_missing="zoning" in context["missing_surfaces"],
        only_federal=context["only_federal_baseline_available"],
    )
    module["fit_assessment"] = _module_fit(
        support_score=support_score,
        blocker_count=len(module["blocking_flags"]),
        status=module["module_status"],
        allow_favorable=(
            context["overall_confirmation_level"] == "parcel_confirmed"
            and "zoning" not in context["missing_surfaces"]
            and not context["only_federal_baseline_available"]
        ),
    )
    module["confidence"] = _module_confidence(
        confidences=confidences,
        support_count=len(module["supporting_signals"]),
        blocker_count=len(module["blocking_flags"]),
        context=context,
        module_status=module["module_status"],
        fit_assessment=module["fit_assessment"],
    )
    module["citation_ids"] = sorted(citations)
    module["evidence_scope_summary"] = _module_scope_summary(context, scopes)
    module["summary"] = _compose_summary(
        label=module["module_label"],
        fit_assessment=module["fit_assessment"],
        supporting_signals=module["supporting_signals"],
        blocking_flags=module["blocking_flags"],
        key_unknowns=module["key_unknowns"],
    )
    return module


def _evaluate_energy_wind(context: dict[str, Any]) -> dict[str, Any]:
    module = _new_module("energy_wind")
    citations: set[str] = set()
    scopes: set[str] = set()
    confidences: list[float] = []
    support_score = 0

    identity_support, identity_fields = _identity_support(context)
    if identity_support:
        module["supporting_signals"].append(identity_support)
        for field in identity_fields:
            _add_field(field, citations, scopes, confidences)

    if context["acreage"] is not None and context["acreage"] >= 80:
        module["supporting_signals"].append(
            f"Available acreage ({context['acreage']:,.0f} acres) is large enough for a directional wind screen."
        )
        support_score += 1
        _add_field(context["fields"]["listing_acreage"], citations, scopes, confidences)

    if _contains_any(context["infra_text"], ("transmission", "substation", "power")):
        module["supporting_signals"].append(
            "Transmission-oriented mapping entry points are available for a future wind diligence pass."
        )
        support_score += 1
        _add_field(context["fields"]["infra_summary"], citations, scopes, confidences)

    if not _contains_any(context["combined_text"], WIND_HINTS):
        module["blocking_flags"].append(
            "No wind-resource or site-specific wind clue is present in the current evidence set."
        )
    module["key_unknowns"] = [
        "No wind-resource screen, turbine setback review, or transmission-queue context has been added yet.",
        "Local permitting and noise/setback treatment for wind are unresolved.",
    ]
    module["economics_inputs_required"] = [
        "wind-resource proxy or modeled generation basis",
        "interconnection cost proxy",
        "local setback and permitting requirements",
    ]
    module["module_status"] = _module_status(
        supporting_count=len(module["supporting_signals"]),
        blocker_count=len(module["blocking_flags"]),
        parcel_identity_blocked=bool(_identity_blockers(context)),
        critical_surface_missing="zoning" in context["missing_surfaces"],
        only_federal=context["only_federal_baseline_available"],
    )
    module["fit_assessment"] = "unknown" if module["module_status"] == "insufficient_data" else "weak"
    module["confidence"] = _module_confidence(
        confidences=confidences,
        support_count=len(module["supporting_signals"]),
        blocker_count=len(module["blocking_flags"]),
        context=context,
        module_status=module["module_status"],
        fit_assessment=module["fit_assessment"],
    )
    module["citation_ids"] = sorted(citations)
    module["evidence_scope_summary"] = _module_scope_summary(context, scopes)
    module["summary"] = _compose_summary(
        label=module["module_label"],
        fit_assessment=module["fit_assessment"],
        supporting_signals=module["supporting_signals"],
        blocking_flags=module["blocking_flags"],
        key_unknowns=module["key_unknowns"],
    )
    return module


def _evaluate_energy_battery(context: dict[str, Any]) -> dict[str, Any]:
    module = _new_module("energy_battery")
    citations: set[str] = set()
    scopes: set[str] = set()
    confidences: list[float] = []
    support_score = 0

    identity_support, identity_fields = _identity_support(context)
    if identity_support:
        module["supporting_signals"].append(identity_support)
        for field in identity_fields:
            _add_field(field, citations, scopes, confidences)

    if context["acreage"] is not None and context["acreage"] >= 10:
        module["supporting_signals"].append(
            f"Available acreage ({context['acreage']:,.0f} acres) is at least large enough for a directional battery-storage screen."
        )
        support_score += 1
        _add_field(context["fields"]["listing_acreage"], citations, scopes, confidences)

    if _contains_any(context["combined_text"], BATTERY_HINTS):
        module["supporting_signals"].append(
            "The request or listing explicitly mentions storage or battery potential."
        )
        support_score += 1
        _add_field(context["fields"]["listing_text"], citations, scopes, confidences)

    if _contains_any(context["infra_text"], ("transmission", "substation", "power", "utility")):
        module["supporting_signals"].append(
            "Power or transmission entry points are visible, which is a necessary but not sufficient battery signal."
        )
        support_score += 1
        _add_field(context["fields"]["infra_summary"], citations, scopes, confidences)
        _add_field(context["fields"]["infra_transmission"], citations, scopes, confidences)
        _add_field(context["fields"]["infra_substation"], citations, scopes, confidences)

    if _contains_any(context["listing_text"], ("road", "frontage", "access")):
        module["supporting_signals"].append(
            "Listing language suggests road frontage or access that could help a logistics-heavy storage concept."
        )
        support_score += 1
        _add_field(context["fields"]["infra_access"], citations, scopes, confidences)
        _add_field(context["fields"]["listing_text"], citations, scopes, confidences)

    zoning_text = _field_text(context["fields"]["planning_zoning"]).lower()
    if not zoning_text:
        module["blocking_flags"].append(
            "No authoritative zoning or use-permit signal is attached for a battery project."
        )
    elif _contains_any(zoning_text, ("agric", "resource intensive")):
        module["blocking_flags"].append(
            "Current zoning clues look agricultural rather than clearly storage-compatible."
        )
        _add_field(context["fields"]["planning_zoning"], citations, scopes, confidences)
    else:
        module["supporting_signals"].append(
            "Current zoning clues do not obviously rule out a storage use, although a direct permit check is still needed."
        )
        support_score += 1
        _add_field(context["fields"]["planning_zoning"], citations, scopes, confidences)

    module["blocking_flags"].extend(_identity_blockers(context))
    if context["only_federal_baseline_available"]:
        module["blocking_flags"].append(
            "Local power-service, zoning, and emergency-response context are too thin for a strong battery call."
        )

    module["key_unknowns"] = [
        "Service capacity, interconnection posture, and upgrade cost are not yet known.",
        "The storage use-permit path and local safety review requirements are unresolved.",
        "Exact access, grading, and fire-response requirements are still unknown.",
    ]
    module["economics_inputs_required"] = [
        "service-capacity or interconnection cost proxy",
        "local permitting and fire-safety requirements",
        "site-civil and access improvement scope",
    ]
    module["module_status"] = _module_status(
        supporting_count=len(module["supporting_signals"]),
        blocker_count=len(module["blocking_flags"]),
        parcel_identity_blocked=bool(_identity_blockers(context)),
        critical_surface_missing="zoning" in context["missing_surfaces"],
        only_federal=context["only_federal_baseline_available"],
    )
    hard_constraint = (
        _contains_any(zoning_text, ("agric", "resource intensive"))
        or context["candidate_set_status"] == "multiple_competing_candidates"
    )
    if hard_constraint and module["module_status"] != "insufficient_data":
        module["fit_assessment"] = "weak"
    else:
        module["fit_assessment"] = _module_fit(
            support_score=support_score,
            blocker_count=len(module["blocking_flags"]),
            status=module["module_status"],
            allow_favorable=False,
        )
    module["confidence"] = _module_confidence(
        confidences=confidences,
        support_count=len(module["supporting_signals"]),
        blocker_count=len(module["blocking_flags"]),
        context=context,
        module_status=module["module_status"],
        fit_assessment=module["fit_assessment"],
    )
    module["citation_ids"] = sorted(citations)
    module["evidence_scope_summary"] = _module_scope_summary(context, scopes)
    module["summary"] = _compose_summary(
        label=module["module_label"],
        fit_assessment=module["fit_assessment"],
        supporting_signals=module["supporting_signals"],
        blocking_flags=module["blocking_flags"],
        key_unknowns=module["key_unknowns"],
    )
    return module


def _evaluate_agriculture_general(context: dict[str, Any]) -> dict[str, Any]:
    module = _new_module("agriculture_general")
    citations: set[str] = set()
    scopes: set[str] = set()
    confidences: list[float] = []
    support_score = 0

    identity_support, identity_fields = _identity_support(context)
    if identity_support:
        module["supporting_signals"].append(identity_support)
        for field in identity_fields:
            _add_field(field, citations, scopes, confidences)

    if context["acreage"] is not None and context["acreage"] >= 20:
        module["supporting_signals"].append(
            f"Available acreage ({context['acreage']:,.0f} acres) is large enough for a general agriculture or grazing screen."
        )
        support_score += 1
        _add_field(context["fields"]["listing_acreage"], citations, scopes, confidences)

    if _has_value(context["fields"]["water_summary"]):
        module["supporting_signals"].append(
            "NRCS-style soil and water attributes provide at least a directional agriculture screen."
        )
        support_score += 2
        _add_field(context["fields"]["water_summary"], citations, scopes, confidences)
        _add_field(context["fields"]["water_soils"], citations, scopes, confidences)

    if context["capability_class"] is not None:
        if context["capability_class"] <= 4:
            module["supporting_signals"].append(
                f"Reported capability class {context['capability_class']} is directionally stronger for working-land uses."
            )
            support_score += 1
        else:
            module["blocking_flags"].append(
                f"Reported capability class {context['capability_class']} points to more marginal nonirrigated land, which weakens intensive agriculture."
            )
        _add_field(context["fields"]["water_summary"], citations, scopes, confidences)
        _add_field(context["fields"]["water_soils"], citations, scopes, confidences)

    if "no intersecting fema flood-hazard zone" in context["env_summary_text"]:
        module["supporting_signals"].append(
            "Point-based flood screening did not show a mapped FEMA flood-hazard hit."
        )
        support_score += 1
        _add_field(context["fields"]["env_flood"], citations, scopes, confidences)

    if _contains_any(context["combined_text"], AG_HINTS):
        module["supporting_signals"].append(
            "The user thesis or prior context explicitly references grazing or agricultural use."
        )
        support_score += 1

    if not _has_value(context["fields"]["water_rights"]):
        module["blocking_flags"].append(
            "No direct groundwater, irrigation-district, or transferable water-right signal is attached yet."
        )

    module["blocking_flags"].extend(_identity_blockers(context))
    module["key_unknowns"] = [
        "Water rights, well depth, pumping cost, or irrigation access are not yet known.",
        "The current evidence does not establish actual carrying capacity, crop mix, or grazing lease terms.",
        "Any ag-exemption or district-specific use constraints remain unresolved.",
    ]
    module["economics_inputs_required"] = [
        "water-source and operating-cost assumption",
        "grazing or crop revenue assumption",
        "property-tax or ag-exemption confirmation",
    ]
    module["module_status"] = _module_status(
        supporting_count=len(module["supporting_signals"]),
        blocker_count=len(module["blocking_flags"]),
        parcel_identity_blocked=bool(_identity_blockers(context)),
        critical_surface_missing=False,
        only_federal=context["only_federal_baseline_available"],
    )
    module["fit_assessment"] = _module_fit(
        support_score=support_score,
        blocker_count=len(module["blocking_flags"]),
        status=module["module_status"],
        allow_favorable=(
            context["capability_class"] is not None
            and context["capability_class"] <= 4
            and not context["only_federal_baseline_available"]
        ),
    )
    module["confidence"] = _module_confidence(
        confidences=confidences,
        support_count=len(module["supporting_signals"]),
        blocker_count=len(module["blocking_flags"]),
        context=context,
        module_status=module["module_status"],
        fit_assessment=module["fit_assessment"],
    )
    module["citation_ids"] = sorted(citations)
    module["evidence_scope_summary"] = _module_scope_summary(context, scopes)
    module["summary"] = _compose_summary(
        label=module["module_label"],
        fit_assessment=module["fit_assessment"],
        supporting_signals=module["supporting_signals"],
        blocking_flags=module["blocking_flags"],
        key_unknowns=module["key_unknowns"],
    )
    return module


def _evaluate_residential_light_development(context: dict[str, Any]) -> dict[str, Any]:
    module = _new_module("residential_light_development")
    citations: set[str] = set()
    scopes: set[str] = set()
    confidences: list[float] = []
    support_score = 0

    acreage = context["acreage"]
    if acreage is not None and 2 <= acreage <= 40:
        module["supporting_signals"].append(
            f"Available acreage ({acreage:,.0f} acres) could support a low-density homesite or small-lot concept."
        )
        support_score += 1
        _add_field(context["fields"]["listing_acreage"], citations, scopes, confidences)

    if _contains_any(context["combined_text"], RESIDENTIAL_HINTS):
        module["supporting_signals"].append(
            "Listing or thesis language suggests at least some residential or light-development intent."
        )
        support_score += 1
        _add_field(context["fields"]["listing_text"], citations, scopes, confidences)

    if _contains_any(context["listing_text"], ("road", "access", "frontage")):
        module["supporting_signals"].append(
            "Road-access language is present, although legal frontage still needs verification."
        )
        support_score += 1
        _add_field(context["fields"]["infra_access"], citations, scopes, confidences)

    if context["max_purchase_price"] is not None and context["ask_price"] is not None:
        if context["ask_price"] <= context["max_purchase_price"]:
            module["supporting_signals"].append(
                "The listing ask fits inside the stated acquisition budget."
            )
            support_score += 1
        _add_field(context["fields"]["listing_asking_price"], citations, scopes, confidences)

    module["blocking_flags"].extend(_identity_blockers(context))
    if "zoning" in context["missing_surfaces"]:
        module["blocking_flags"].append(
            "No parcel-specific zoning, minimum-lot-size, or subdivision signal is attached yet."
        )
    if context["only_federal_baseline_available"]:
        module["blocking_flags"].append(
            "Only federal baseline plus listing context is available, which is too thin for a confident development screen."
        )
    if not _has_value(context["fields"]["infra_water"]) and not _has_value(context["fields"]["infra_septic"]):
        module["blocking_flags"].append(
            "No water-service, septic, or wastewater proxy is attached for residential use."
        )

    module["key_unknowns"] = [
        "Minimum lot size, subdivision pathway, and local residential zoning compatibility remain unresolved.",
        "Water, septic, and frontage requirements are still unknown.",
        "Boundary-based hazard checks would be needed before treating a buildability thesis as reliable.",
    ]
    module["economics_inputs_required"] = [
        "subdivision or homesite improvement budget",
        "water / septic installation cost proxy",
        "entitlement and frontage assumption",
    ]
    module["module_status"] = _module_status(
        supporting_count=len(module["supporting_signals"]),
        blocker_count=len(module["blocking_flags"]),
        parcel_identity_blocked=bool(_identity_blockers(context)),
        critical_surface_missing="zoning" in context["missing_surfaces"],
        only_federal=context["only_federal_baseline_available"],
    )
    module["fit_assessment"] = _module_fit(
        support_score=support_score,
        blocker_count=len(module["blocking_flags"]),
        status=module["module_status"],
        allow_favorable=False,
    )
    module["confidence"] = _module_confidence(
        confidences=confidences,
        support_count=len(module["supporting_signals"]),
        blocker_count=len(module["blocking_flags"]),
        context=context,
        module_status=module["module_status"],
        fit_assessment=module["fit_assessment"],
    )
    module["citation_ids"] = sorted(citations)
    module["evidence_scope_summary"] = _module_scope_summary(context, scopes)
    module["summary"] = _compose_summary(
        label=module["module_label"],
        fit_assessment=module["fit_assessment"],
        supporting_signals=module["supporting_signals"],
        blocking_flags=module["blocking_flags"],
        key_unknowns=module["key_unknowns"],
    )
    return module


def _evaluate_industrial_storage(context: dict[str, Any]) -> dict[str, Any]:
    module = _new_module("industrial_storage")
    citations: set[str] = set()
    scopes: set[str] = set()
    confidences: list[float] = []
    support_score = 0

    if context["acreage"] is not None and context["acreage"] >= 20:
        module["supporting_signals"].append(
            f"Available acreage ({context['acreage']:,.0f} acres) is large enough for at least a directional industrial or storage-yard screen."
        )
        support_score += 1
        _add_field(context["fields"]["listing_acreage"], citations, scopes, confidences)

    if _contains_any(context["combined_text"], INDUSTRIAL_HINTS):
        module["supporting_signals"].append(
            "The request or listing carries explicit industrial / storage language."
        )
        support_score += 1
        _add_field(context["fields"]["listing_text"], citations, scopes, confidences)

    if _contains_any(context["infra_text"], ("road", "frontage", "highway", "logistics", "transmission", "power")):
        module["supporting_signals"].append(
            "Access or power-related context is visible, which helps a next-step industrial screen."
        )
        support_score += 1
        _add_field(context["fields"]["infra_access"], citations, scopes, confidences)
        _add_field(context["fields"]["infra_logistics"], citations, scopes, confidences)
        _add_field(context["fields"]["infra_summary"], citations, scopes, confidences)

    zoning_text = _field_text(context["fields"]["planning_zoning"]).lower()
    if not zoning_text:
        module["blocking_flags"].append(
            "No authoritative industrial-compatibility or zoning signal is attached yet."
        )
    elif _contains_any(zoning_text, ("agric", "resource intensive")):
        module["blocking_flags"].append(
            "Current zoning clues lean agricultural rather than industrial / storage oriented."
        )
        _add_field(context["fields"]["planning_zoning"], citations, scopes, confidences)
    else:
        module["supporting_signals"].append(
            "Current zoning clues do not obviously conflict with a storage-oriented use."
        )
        support_score += 1
        _add_field(context["fields"]["planning_zoning"], citations, scopes, confidences)

    module["blocking_flags"].extend(_identity_blockers(context))
    if context["only_federal_baseline_available"]:
        module["blocking_flags"].append(
            "Local logistics, zoning, and utility footing is still too thin for a strong industrial screen."
        )

    module["key_unknowns"] = [
        "Utility service capacity, truck access quality, and local industrial use approval remain unresolved.",
        "The current evidence does not establish parcel shape, buffering, or adjacency suitability.",
        "Environmental and stormwater requirements for a hardscape use are still unknown.",
    ]
    module["economics_inputs_required"] = [
        "site-civil and access improvement budget",
        "utility / power-service upgrade cost proxy",
        "zoning and permit pathway assumption",
    ]
    module["module_status"] = _module_status(
        supporting_count=len(module["supporting_signals"]),
        blocker_count=len(module["blocking_flags"]),
        parcel_identity_blocked=bool(_identity_blockers(context)),
        critical_surface_missing="zoning" in context["missing_surfaces"],
        only_federal=context["only_federal_baseline_available"],
    )
    hard_constraint = (
        _contains_any(zoning_text, ("agric", "resource intensive"))
        or context["candidate_set_status"] == "multiple_competing_candidates"
    )
    if hard_constraint and module["module_status"] != "insufficient_data":
        module["fit_assessment"] = "weak"
    else:
        module["fit_assessment"] = _module_fit(
            support_score=support_score,
            blocker_count=len(module["blocking_flags"]),
            status=module["module_status"],
            allow_favorable=False,
        )
    module["confidence"] = _module_confidence(
        confidences=confidences,
        support_count=len(module["supporting_signals"]),
        blocker_count=len(module["blocking_flags"]),
        context=context,
        module_status=module["module_status"],
        fit_assessment=module["fit_assessment"],
    )
    module["citation_ids"] = sorted(citations)
    module["evidence_scope_summary"] = _module_scope_summary(context, scopes)
    module["summary"] = _compose_summary(
        label=module["module_label"],
        fit_assessment=module["fit_assessment"],
        supporting_signals=module["supporting_signals"],
        blocking_flags=module["blocking_flags"],
        key_unknowns=module["key_unknowns"],
    )
    return module


def _evaluate_recreational_rural_hold(context: dict[str, Any]) -> dict[str, Any]:
    module = _new_module("recreational_rural_hold")
    citations: set[str] = set()
    scopes: set[str] = set()
    confidences: list[float] = []
    support_score = 0

    identity_support, identity_fields = _identity_support(context)
    if identity_support:
        module["supporting_signals"].append(identity_support)
        support_score += 1 if context["overall_confirmation_level"] == "parcel_confirmed" else 0
        for field in identity_fields:
            _add_field(field, citations, scopes, confidences)

    acreage = context["acreage"]
    if acreage is not None and acreage >= 20:
        module["supporting_signals"].append(
            f"Available acreage ({acreage:,.0f} acres) supports a rural hold or recreation-style land-bank thesis."
        )
        support_score += 1
        _add_field(context["fields"]["listing_acreage"], citations, scopes, confidences)

    if _contains_any(context["combined_text"], ("road", "access", "frontage")):
        module["supporting_signals"].append(
            "Road or access language is present, which helps a recreation or hold thesis even though legal access is still unconfirmed."
        )
        support_score += 1
        _add_field(context["fields"]["infra_access"], citations, scopes, confidences)

    if _contains_any(context["combined_text"], ("rural hold", "hold", "desert views", "views", "recreation", "hunting", "retreat")):
        module["supporting_signals"].append(
            "The listing or thesis directly supports a rural hold / recreation framing."
        )
        support_score += 1
        _add_field(context["fields"]["listing_text"], citations, scopes, confidences)

    if context["max_purchase_price"] is not None and context["ask_price"] is not None:
        if context["ask_price"] <= context["max_purchase_price"]:
            module["supporting_signals"].append(
                "The current listing ask fits inside the acquisition budget for a patient hold strategy."
            )
            support_score += 1
        _add_field(context["fields"]["listing_asking_price"], citations, scopes, confidences)
        _add_field(context["fields"]["market_ask_per_acre"], citations, scopes, confidences)

    if "no intersecting fema flood-hazard zone" in context["env_summary_text"] or "no mapped wetland polygon" in context["env_summary_text"]:
        module["supporting_signals"].append(
            "Point-based environmental screens do not show an obvious flood or wetlands burden at the sampled location."
        )
        support_score += 1
        _add_field(context["fields"]["env_flood"], citations, scopes, confidences)
        _add_field(context["fields"]["env_wetlands"], citations, scopes, confidences)

    module["blocking_flags"].extend(_identity_blockers(context))
    if context["only_federal_baseline_available"]:
        module["blocking_flags"].append(
            "Local parcel, tax, and market footing is still thin, so the hold thesis stays mostly directional."
        )

    module["key_unknowns"] = [
        "Carrying costs, tax burden, and resale liquidity are not yet well constrained.",
        "If the hold thesis depends on future building rights, zoning and utility footing remain unresolved.",
        "Boundary-based access and environmental confirmation would still be needed before a final buy call.",
    ]
    module["economics_inputs_required"] = [
        "property-tax and annual carry-cost proxy",
        "expected hold period and exit liquidity assumption",
        "minimal access or gate improvement allowance",
    ]
    module["module_status"] = _module_status(
        supporting_count=len(module["supporting_signals"]),
        blocker_count=len(module["blocking_flags"]),
        parcel_identity_blocked=bool(_identity_blockers(context)),
        critical_surface_missing=False,
        only_federal=context["only_federal_baseline_available"],
    )
    module["fit_assessment"] = _module_fit(
        support_score=support_score,
        blocker_count=len(module["blocking_flags"]),
        status=module["module_status"],
        allow_favorable=(
            context["overall_confirmation_level"] == "parcel_confirmed"
            and not context["only_federal_baseline_available"]
        ),
    )
    module["confidence"] = _module_confidence(
        confidences=confidences,
        support_count=len(module["supporting_signals"]),
        blocker_count=len(module["blocking_flags"]),
        context=context,
        module_status=module["module_status"],
        fit_assessment=module["fit_assessment"],
    )
    module["citation_ids"] = sorted(citations)
    module["evidence_scope_summary"] = _module_scope_summary(context, scopes)
    module["summary"] = _compose_summary(
        label=module["module_label"],
        fit_assessment=module["fit_assessment"],
        supporting_signals=module["supporting_signals"],
        blocking_flags=module["blocking_flags"],
        key_unknowns=module["key_unknowns"],
    )
    return module


MODULE_EVALUATORS = {
    "energy_solar": _evaluate_energy_solar,
    "energy_wind": _evaluate_energy_wind,
    "energy_battery": _evaluate_energy_battery,
    "agriculture_general": _evaluate_agriculture_general,
    "residential_light_development": _evaluate_residential_light_development,
    "industrial_storage": _evaluate_industrial_storage,
    "recreational_rural_hold": _evaluate_recreational_rural_hold,
}


def sort_modules(modules: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return sorted(
        modules,
        key=lambda module: (
            FIT_ORDER.get(module["fit_assessment"], 99),
            MODULE_STATUS_ORDER.get(module["module_status"], 99),
            -float(module.get("confidence", 0.0)),
            MODULE_ORDER.get(module["module_id"], 999),
        ),
    )


def _economics_assumptions(context: dict[str, Any], modules: list[dict[str, Any]]) -> list[dict[str, Any]]:
    assumptions: list[dict[str, Any]] = []
    if context["ask_price"] is not None:
        assumptions.append(
            {
                "name": "Listing ask placeholder",
                "value": f"${context['ask_price']:,.2f}",
                "source": "listing_source",
            }
        )
    if context["acreage"] is not None:
        assumptions.append(
            {
                "name": "Working acreage placeholder",
                "value": f"{context['acreage']:,.0f} acres",
                "source": "listing_source" if _has_value(context["fields"]["listing_acreage"]) else "public_record",
            }
        )
    hold_period = context["hold_period"]
    if hold_period.get("min_years") or hold_period.get("max_years"):
        min_years = hold_period.get("min_years")
        max_years = hold_period.get("max_years")
        if min_years and max_years:
            value = f"{min_years}-{max_years} year target hold window"
        else:
            value = f"{min_years or max_years} year target hold window"
        assumptions.append(
            {
                "name": "Hold period assumption",
                "value": value,
                "source": "user_assumption",
            }
        )
    if context["max_purchase_price"] is not None:
        assumptions.append(
            {
                "name": "Budget ceiling",
                "value": f"${context['max_purchase_price']:,.2f}",
                "source": "user_assumption",
            }
        )
    if modules:
        top_module = sort_modules(modules)[0]
        assumptions.append(
            {
                "name": "Lead screening module",
                "value": f"{top_module['module_label']} ({top_module['fit_assessment']})",
                "source": "derived_inference",
            }
        )
    return assumptions[:5]


def _economics_carry_costs(context: dict[str, Any], modules: list[dict[str, Any]]) -> dict[str, Any]:
    hold_period = context["hold_period"]
    carry_costs = {
        "holding_period_context": evidence_field(
            (
                f"{hold_period.get('min_years')}-{hold_period.get('max_years')} year hold target"
                if hold_period.get("min_years") and hold_period.get("max_years")
                else None
            ),
            status="estimated" if hold_period.get("min_years") or hold_period.get("max_years") else "missing",
            confidence=0.55 if hold_period.get("min_years") or hold_period.get("max_years") else 0.0,
            evidence_scope="inferred" if hold_period.get("min_years") or hold_period.get("max_years") else "unresolved",
            notes="Carry framing currently reflects user-stated hold timing rather than parcel-specific tax roll evidence."
            if hold_period.get("min_years") or hold_period.get("max_years")
            else "No hold-period signal is available.",
        ),
        "property_tax_and_assessment": evidence_field(
            notes="Property-tax and assessment carry costs remain unresolved until tax-roll extraction is added."
        ),
    }
    if any(module["module_id"] == "recreational_rural_hold" for module in modules):
        carry_costs["liquidity_and_resale_horizon"] = evidence_field(
            "Rural hold exit timing depends on local resale demand rather than a modeled liquidity curve.",
            status="estimated",
            confidence=0.35,
            evidence_scope="inferred",
            notes="Directional only; no comps-backed liquidity model is included yet.",
        )
    return carry_costs


def _economics_capex(context: dict[str, Any], modules: list[dict[str, Any]]) -> dict[str, Any]:
    capex: dict[str, Any] = {}
    if any(module["module_id"] in {"energy_solar", "energy_battery"} for module in modules):
        capex["utility_or_interconnection"] = evidence_field(
            "Utility or transmission entry points are visible, but extension and interconnection cost remain unresolved.",
            status="estimated" if _contains_any(context["infra_text"], ("transmission", "power", "utility")) else "missing",
            confidence=0.42 if _contains_any(context["infra_text"], ("transmission", "power", "utility")) else 0.0,
            evidence_scope="geography_only" if _contains_any(context["infra_text"], ("transmission", "power", "utility")) else "unresolved",
            citation_ids=sorted_unique_strings(
                _field_citation_ids(context["fields"]["infra_summary"])
                + _field_citation_ids(context["fields"]["infra_transmission"])
                + _field_citation_ids(context["fields"]["infra_power"])
            ),
            notes="No interconnection study, queue status, or service-capacity estimate is included in Step 9.",
        )
    if any(module["module_id"] == "agriculture_general" for module in modules):
        capex["water_or_well_improvements"] = evidence_field(
            "Water development cost is unresolved pending direct well, groundwater, or irrigation evidence.",
            status="estimated" if _has_value(context["fields"]["water_summary"]) else "missing",
            confidence=0.35 if _has_value(context["fields"]["water_summary"]) else 0.0,
            evidence_scope=_field_scope(context["fields"]["water_summary"]) if _has_value(context["fields"]["water_summary"]) else "unresolved",
            citation_ids=_field_citation_ids(context["fields"]["water_summary"]),
            notes="Directional only; no well depth, drilling quote, or delivery-right evidence is attached.",
        )
    if any(module["module_id"] == "residential_light_development" for module in modules):
        capex["water_septic_and_subdivision"] = evidence_field(
            "Residential improvement scope depends on water, septic, frontage, and subdivision rules that are still unresolved.",
            status="estimated",
            confidence=0.3,
            evidence_scope="unresolved",
            notes="No utility-extension, septic, or platting cost model is included yet.",
        )
    capex["entitlement_and_local_permits"] = evidence_field(
        "Entitlement, permit, and local approval costs remain unresolved until zoning and local pathway checks are direct.",
        status="estimated",
        confidence=0.32,
        evidence_scope="county_level" if _has_value(context["fields"]["planning_summary"]) else "unresolved",
        citation_ids=_field_citation_ids(context["fields"]["planning_summary"]),
        notes="Step 9 uses screening-grade planning signals only.",
    )
    return capex


def _economics_scenarios(
    context: dict[str, Any],
    modules: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    scenarios: list[dict[str, Any]] = []
    for module in sort_modules(modules):
        if module["module_status"] == "insufficient_data":
            continue
        scenario = {
            "module_id": module["module_id"],
            "scenario_name": module["module_label"],
            "description": module["summary"],
            "key_inputs": module["economics_inputs_required"][:3],
            "notes": (
                "Directional only; no final underwriting, IRR, NPV, or revenue model is produced in Step 9."
            ),
        }
        if module["module_id"] == "energy_solar":
            scenario["scenario_name"] = "Solar option value / ground lease"
        elif module["module_id"] == "energy_battery":
            scenario["scenario_name"] = "Battery storage option value"
        elif module["module_id"] == "agriculture_general":
            scenario["scenario_name"] = "Working-land / grazing hold"
        elif module["module_id"] == "residential_light_development":
            scenario["scenario_name"] = "Low-density homesite or small subdivision"
        elif module["module_id"] == "industrial_storage":
            scenario["scenario_name"] = "Industrial yard / storage use"
        elif module["module_id"] == "recreational_rural_hold":
            scenario["scenario_name"] = "Rural hold / recreation resale"
        scenarios.append(scenario)
    if not scenarios and context["ask_price"] is not None:
        scenarios.append(
            {
                "module_id": "recreational_rural_hold",
                "scenario_name": "Hold / exit placeholder",
                "description": "Only a seller-facing acquisition basis is currently available.",
                "key_inputs": [
                    "purchase basis",
                    "carry cost proxy",
                    "exit timing assumption",
                ],
                "notes": "Directional only; no module earned a stronger economics scenario yet.",
            }
        )
    return scenarios[:5]


def _economics_summary(context: dict[str, Any], modules: list[dict[str, Any]]) -> list[str]:
    summary: list[str] = []
    ranked = sort_modules(modules)
    if ranked:
        top = ranked[0]
        summary.append(
            f"Lead module: {top['module_label']} is {top['fit_assessment'].replace('_', ' ')} at screening grade."
        )
    if context["ask_price"] is not None and context["acreage"] is not None:
        summary.append(
            f"Current acquisition basis relies on a listing ask of ${context['ask_price']:,.2f} for about {context['acreage']:,.0f} acres."
        )
    else:
        summary.append(
            "No active listing price is attached, so economics remain scenario framing rather than purchase underwriting."
        )
    if context["only_federal_baseline_available"]:
        summary.append(
            "Local footing is thin, so economics should be treated as directional only."
        )
    return summary[:4]


def _economics_limitations(context: dict[str, Any], modules: list[dict[str, Any]]) -> list[str]:
    limitations = [
        "Step 9 does not produce final underwriting, IRR, NPV, or guaranteed ROI outputs.",
        "Interconnection feasibility, entitlement certainty, title, and legal water-rights analysis remain outside this step.",
    ]
    if context["candidate_set_status"] == "multiple_competing_candidates":
        limitations.append(
            "Competing parcel candidates materially limit the reliability of parcel-specific economics."
        )
    elif context["overall_confirmation_level"] != "parcel_confirmed":
        limitations.append(
            "Parcel identity is not yet parcel-confirmed, which weakens parcel-specific economics."
        )
    if context["only_federal_baseline_available"]:
        limitations.append(
            "Only federal baseline plus listing context is available, so local development cost assumptions are especially thin."
        )
    if not any(module["fit_assessment"] in {"favorable", "mixed"} for module in modules):
        limitations.append(
            "No module currently has enough support for a strong directional-economics framing."
        )
    return limitations[:5]


def _directional_economics(
    context: dict[str, Any],
    modules: list[dict[str, Any]],
) -> dict[str, Any]:
    active_module_ids = [module["module_id"] for module in modules]
    has_price_basis = context["ask_price"] is not None and context["acreage"] is not None
    has_meaningful_module = any(
        module["fit_assessment"] in {"favorable", "mixed", "weak"} and module["module_status"] != "insufficient_data"
        for module in modules
    )
    if has_price_basis and has_meaningful_module and not context["only_federal_baseline_available"]:
        status = "available"
    elif has_price_basis or has_meaningful_module:
        status = "limited"
    else:
        status = "not_enough_data"

    if status == "available":
        basis = (
            "Directional economics can frame screening-grade acquisition and exit cases from the current listing basis plus module outputs, "
            "but they still stop short of underwriting."
        )
    elif status == "limited":
        basis = (
            "Directional economics are limited to scenario framing from the current thesis modules, listing basis, and broad public-data signals."
        )
    else:
        basis = (
            "Current evidence is too thin for meaningful directional economics beyond explicit limitations and missing inputs."
        )

    return {
        "status": status,
        "basis": basis,
        "active_module_ids": active_module_ids,
        "assumptions": _economics_assumptions(context, modules),
        "carry_costs": _economics_carry_costs(context, modules),
        "improvement_capex_proxies": _economics_capex(context, modules),
        "revenue_or_exit_cases": _economics_scenarios(context, modules),
        "scenario_summary": _economics_summary(context, modules),
        "limitations": _economics_limitations(context, modules),
    }


def _module_unknowns(modules: list[dict[str, Any]]) -> list[dict[str, Any]]:
    unknowns: list[dict[str, Any]] = []
    for module in modules:
        module_id = module["module_id"]
        if module_id == "energy_solar":
            unknowns.append(
                {
                    "unknown_id": "module_energy_solar_interconnection",
                    "question": "What utility, substation, or interconnection path could actually support the solar thesis?",
                    "why_it_matters": "Solar value can change materially once real power-delivery constraints are known.",
                    "recommended_next_source": "interconnection",
                    "blocking": module["fit_assessment"] in {"weak", "unknown"},
                }
            )
        elif module_id == "energy_battery":
            unknowns.append(
                {
                    "unknown_id": "module_energy_battery_service_capacity",
                    "question": "Is there realistic service-capacity and permit footing for a battery-storage concept?",
                    "why_it_matters": "Battery feasibility is highly sensitive to utility capacity and permitting.",
                    "recommended_next_source": "utility_capacity",
                    "blocking": True,
                }
            )
        elif module_id == "agriculture_general":
            unknowns.append(
                {
                    "unknown_id": "module_agriculture_water",
                    "question": "What groundwater, well, irrigation, or transferable water-right footing exists for agricultural use?",
                    "why_it_matters": "Agriculture outcomes can change materially once water supply and cost are known.",
                    "recommended_next_source": "water",
                    "blocking": module["fit_assessment"] in {"weak", "unknown"},
                }
            )
        elif module_id == "residential_light_development":
            unknowns.append(
                {
                    "unknown_id": "module_residential_entitlement",
                    "question": "What local zoning, frontage, water, and septic requirements govern a homesite or subdivision concept?",
                    "why_it_matters": "Residential feasibility can change materially once entitlement and utility rules are known.",
                    "recommended_next_source": "zoning",
                    "blocking": True,
                }
            )
        elif module_id == "industrial_storage":
            unknowns.append(
                {
                    "unknown_id": "module_industrial_zoning_capacity",
                    "question": "What zoning and utility-capacity footing would support industrial or outdoor-storage use here?",
                    "why_it_matters": "Industrial feasibility is highly sensitive to local zoning, access, and power-service limits.",
                    "recommended_next_source": "zoning",
                    "blocking": True,
                }
            )
        elif module_id == "recreational_rural_hold":
            unknowns.append(
                {
                    "unknown_id": "module_rural_hold_carry",
                    "question": "What annual carry cost and local exit-liquidity profile should be assumed for a rural hold?",
                    "why_it_matters": "A hold thesis can weaken quickly if taxes, carry costs, or exit timing are worse than expected.",
                    "recommended_next_source": "market_context",
                    "blocking": False,
                }
            )
    deduped: list[dict[str, Any]] = []
    seen_ids: set[str] = set()
    for unknown in unknowns:
        if unknown["unknown_id"] in seen_ids:
            continue
        seen_ids.add(unknown["unknown_id"])
        deduped.append(unknown)
    return deduped


def _module_next_actions(modules: list[dict[str, Any]]) -> list[dict[str, Any]]:
    actions: list[dict[str, Any]] = []
    priority = 20
    for module in sort_modules(modules):
        module_id = module["module_id"]
        if module_id == "energy_solar":
            actions.append(
                {
                    "priority": priority,
                    "action": "verify_solar_zoning_and_interconnection",
                    "reason": "Solar screening remains gated by local compatibility and real utility-delivery footing.",
                }
            )
        elif module_id == "energy_battery":
            actions.append(
                {
                    "priority": priority,
                    "action": "verify_battery_permit_and_service_capacity",
                    "reason": "Battery screening remains gated by local permits, safety review, and service-capacity questions.",
                }
            )
        elif module_id == "agriculture_general":
            actions.append(
                {
                    "priority": priority,
                    "action": "verify_ag_water_source_and_cost",
                    "reason": "Agriculture screening remains gated by water availability, cost, and any irrigation-right context.",
                }
            )
        elif module_id == "residential_light_development":
            actions.append(
                {
                    "priority": priority,
                    "action": "verify_residential_zoning_and_utilities",
                    "reason": "Residential screening needs local zoning, frontage, water, and septic confirmation before it can harden.",
                }
            )
        elif module_id == "industrial_storage":
            actions.append(
                {
                    "priority": priority,
                    "action": "verify_industrial_zoning_and_logistics",
                    "reason": "Industrial screening needs local zoning, truck access, and power-service confirmation.",
                }
            )
        elif module_id == "recreational_rural_hold":
            actions.append(
                {
                    "priority": priority,
                    "action": "estimate_rural_hold_carry_and_exit",
                    "reason": "A rural-hold thesis improves materially once carry costs and exit liquidity are bounded.",
                }
            )
        priority += 1
    return actions


def evaluate_use_case_modules(
    request: dict[str, Any],
    parcel_memo: dict[str, Any],
    *,
    parcel_candidates: dict[str, Any] | None = None,
    coverage_assessment: dict[str, Any] | None = None,
    jurisdiction_context: dict[str, Any] | None = None,
) -> dict[str, Any]:
    del parcel_candidates, jurisdiction_context
    module_ids = select_module_ids(request)
    context = _module_context(request, parcel_memo, coverage_assessment)
    modules = [MODULE_EVALUATORS[module_id](context) for module_id in module_ids]
    modules = sort_modules(modules)
    return {
        "use_case_modules": modules,
        "directional_economics": _directional_economics(context, modules),
        "derived_unknowns": _module_unknowns(modules),
        "derived_next_actions": _module_next_actions(modules),
    }
